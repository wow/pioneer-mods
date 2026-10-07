//! Pure, in-memory recipe engine: compatibility gate, bounded apply, and post-apply verification.
//!
//! The engine performs no file-system I/O so that every front-end (CLI, future WASM/desktop)
//! shares the same safety checks. Callers own reading input and writing output.

use crate::error::{OperationRegion, PatchEngineError};
use crate::identity::sha256_hex;
use patch_schema::{PatchOperation, RecipeManifest, SchemaValidationError, SupportedFirmware};
use std::ops::Range;

/// Result of a successful recipe application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOutcome<'m> {
    /// Manifest target that matched the input identity (size + SHA-256).
    pub target: &'m SupportedFirmware,
    /// SHA-256 of the exact input bytes that were patched.
    pub input_sha256_hex: String,
    /// Patched bytes; same length as the input.
    pub output_bytes: Vec<u8>,
    /// SHA-256 of `output_bytes`.
    pub output_sha256_hex: String,
}

/// Applies `manifest` to `input_bytes`.
///
/// Steps, all of which must pass before any output is returned:
/// 1. the manifest passes [`RecipeManifest::validate`],
/// 2. the input matches a declared target on size and SHA-256 (hash computed from `input_bytes`),
/// 3. every operation range is in bounds and destination ranges do not overlap,
/// 4. operations are applied; `owner_copy_window` always reads from the original input,
/// 5. the output differs from the input only inside declared destination ranges,
/// 6. the output SHA-256 equals the target's `expected_output_sha256`, when declared.
///
/// # Errors
///
/// Returns a [`PatchEngineError`] describing the first failed check.
pub fn apply_recipe<'m>(
    manifest: &'m RecipeManifest,
    input_bytes: &[u8],
) -> Result<PatchOutcome<'m>, PatchEngineError> {
    manifest.validate()?;

    let input_sha256_hex = sha256_hex(input_bytes);
    let input_size = input_bytes.len() as u64;
    let target = manifest
        .matching_target(input_size, &input_sha256_hex)
        .ok_or_else(|| PatchEngineError::IncompatibleInput {
            recipe_id: manifest.recipe_id.clone(),
            size_bytes: input_size,
            sha256_hex: input_sha256_hex.clone(),
        })?;

    let planned = plan_operations(&manifest.operations, input_bytes.len())?;
    let VerifiedOutput(output_bytes) = apply_and_verify(input_bytes, &planned)?;

    let output_sha256_hex = sha256_hex(&output_bytes);
    if let Some(expected) = &target.expected_output_sha256
        && !output_sha256_hex.eq_ignore_ascii_case(expected)
    {
        return Err(PatchEngineError::OutputHashMismatch {
            recipe_id: manifest.recipe_id.clone(),
            expected: expected.clone(),
            actual: output_sha256_hex,
        });
    }

    Ok(PatchOutcome {
        target,
        input_sha256_hex,
        output_bytes,
        output_sha256_hex,
    })
}

/// Checks that `output_bytes` differs from `input_bytes` only inside `declared_ranges`.
///
/// Ranges may be given in any order. Lengths must be equal.
///
/// # Errors
///
/// [`PatchEngineError::OutputLengthChanged`] or
/// [`PatchEngineError::MutationOutsideDeclaredRegions`] for the first offending byte.
pub fn verify_bounded_diff(
    input_bytes: &[u8],
    output_bytes: &[u8],
    declared_ranges: &[Range<usize>],
) -> Result<(), PatchEngineError> {
    if input_bytes.len() != output_bytes.len() {
        return Err(PatchEngineError::OutputLengthChanged {
            input_len: input_bytes.len(),
            output_len: output_bytes.len(),
        });
    }

    let mut ranges = declared_ranges.to_vec();
    ranges.sort_by_key(|range| (range.start, range.end));

    let mut range_idx = 0usize;
    for (byte_offset, (input_byte, output_byte)) in input_bytes.iter().zip(output_bytes).enumerate()
    {
        if input_byte == output_byte {
            continue;
        }
        while ranges
            .get(range_idx)
            .is_some_and(|range| byte_offset >= range.end)
        {
            range_idx += 1;
        }
        let inside = ranges
            .get(range_idx)
            .is_some_and(|range| range.contains(&byte_offset));
        if !inside {
            return Err(PatchEngineError::MutationOutsideDeclaredRegions { byte_offset });
        }
    }

    Ok(())
}

/// Output bytes that passed [`verify_bounded_diff`]. Only [`apply_and_verify`] constructs it,
/// so `apply_recipe` cannot return output that skipped the bounded-diff check.
struct VerifiedOutput(Vec<u8>);

fn apply_and_verify(
    input_bytes: &[u8],
    planned: &[PlannedOperation<'_>],
) -> Result<VerifiedOutput, PatchEngineError> {
    let output_bytes = apply_planned(input_bytes, planned);
    let destination_ranges: Vec<Range<usize>> =
        planned.iter().map(|op| op.destination.clone()).collect();
    verify_bounded_diff(input_bytes, &output_bytes, &destination_ranges)?;
    Ok(VerifiedOutput(output_bytes))
}

/// One operation with all ranges bounds-checked against the input length.
struct PlannedOperation<'a> {
    operation_index: usize,
    destination: Range<usize>,
    source: PlannedSource<'a>,
}

enum PlannedSource<'a> {
    Literal(&'a [u8]),
    Input(Range<usize>),
}

fn plan_operations(
    operations: &[PatchOperation],
    total_len: usize,
) -> Result<Vec<PlannedOperation<'_>>, PatchEngineError> {
    let mut planned = Vec::with_capacity(operations.len());
    for (operation_index, operation) in operations.iter().enumerate() {
        let planned_operation = match operation {
            PatchOperation::WriteSpan(span) => {
                let destination = checked_range(
                    span.offset,
                    span.length,
                    total_len,
                    operation_index,
                    OperationRegion::WriteSpanDestination,
                )?;
                // Also enforced by `RecipeManifest::validate`; re-checked so apply cannot panic.
                if span.bytes.len() != destination.len() {
                    return Err(SchemaValidationError::WriteSpanLengthMismatch.into());
                }
                PlannedOperation {
                    operation_index,
                    destination,
                    source: PlannedSource::Literal(&span.bytes),
                }
            }
            PatchOperation::OwnerCopyWindow(window) => PlannedOperation {
                operation_index,
                destination: checked_range(
                    window.destination_offset,
                    window.length,
                    total_len,
                    operation_index,
                    OperationRegion::OwnerCopyDestination,
                )?,
                source: PlannedSource::Input(checked_range(
                    window.source_offset,
                    window.length,
                    total_len,
                    operation_index,
                    OperationRegion::OwnerCopySource,
                )?),
            },
        };
        planned.push(planned_operation);
    }

    reject_overlapping_destinations(&planned)?;
    Ok(planned)
}

fn reject_overlapping_destinations(
    planned: &[PlannedOperation<'_>],
) -> Result<(), PatchEngineError> {
    let mut by_start: Vec<&PlannedOperation<'_>> = planned.iter().collect();
    by_start.sort_by_key(|op| (op.destination.start, op.destination.end, op.operation_index));
    for pair in by_start.windows(2) {
        let [previous, current] = pair else {
            continue;
        };
        if current.destination.start < previous.destination.end {
            return Err(PatchEngineError::OverlappingDestinations {
                operation_index: current.operation_index,
                start: current.destination.start,
                end: current.destination.end,
                other_operation_index: previous.operation_index,
                other_start: previous.destination.start,
                other_end: previous.destination.end,
            });
        }
    }
    Ok(())
}

/// Applies bounds-checked operations. Sources are read from the original input, so the result
/// does not depend on operation order (destinations are already known not to overlap).
fn apply_planned(input_bytes: &[u8], planned: &[PlannedOperation<'_>]) -> Vec<u8> {
    let mut output_bytes = input_bytes.to_vec();
    for op in planned {
        let source = match &op.source {
            PlannedSource::Literal(bytes) => *bytes,
            PlannedSource::Input(range) => &input_bytes[range.clone()],
        };
        output_bytes[op.destination.clone()].copy_from_slice(source);
    }
    output_bytes
}

fn checked_range(
    offset: u64,
    length: u64,
    total_len: usize,
    operation_index: usize,
    region: OperationRegion,
) -> Result<Range<usize>, PatchEngineError> {
    let out_of_bounds = || PatchEngineError::RangeOutOfBounds {
        operation_index,
        region,
        offset,
        length,
        input_size: total_len,
    };
    let start = usize::try_from(offset).map_err(|_| out_of_bounds())?;
    let span_len = usize::try_from(length).map_err(|_| out_of_bounds())?;
    let end = start.checked_add(span_len).ok_or_else(out_of_bounds)?;
    if end > total_len {
        return Err(out_of_bounds());
    }
    Ok(start..end)
}
