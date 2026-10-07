use anyhow::{Context, Result};
use clap::ValueEnum;
use patch_core::upd::{ImageReport, MAX_IMAGE_LEN, MAX_TOTAL_IMAGE_LEN, UpdSummary};
use patch_core::{
    FirmwareIdentity, UpdContainer, identify_firmware, parse_upd, read_firmware, xdj700,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(clap::Args, Debug)]
pub struct InspectArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    pub input: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Also parse and validate the .UPD container structure (documents, CRCs, S-records).
    #[arg(long, default_value_t = false)]
    pub structure: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Serialize)]
struct InspectReport {
    #[serde(flatten)]
    identity: FirmwareIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<UpdSummary>,
    /// `true` once re-serializing the parsed container reproduced the input byte-for-byte
    /// (present only with `--structure`; a failed roundtrip aborts the command).
    #[serde(skip_serializing_if = "Option::is_none")]
    roundtrip_verified: Option<bool>,
    /// XDJ-700 compressed application section (MAIN image offset 0x40000), when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    application: Option<ApplicationReport>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ApplicationReport {
    Decoded {
        offset: String,
        compressed_len: usize,
        checksum: String,
        decoded_len: usize,
        decoded_sha256: String,
    },
    /// The section failed verification; reported rather than failing the structure report.
    Invalid { reason: String },
}

impl ApplicationReport {
    fn from_container(container: &UpdContainer) -> Option<Self> {
        if !xdj700::is_xdj700(container) {
            return None;
        }
        Some(match xdj700::decode_application(container) {
            Ok(section) => Self::Decoded {
                offset: format!("0x{:X}", section.offset()),
                compressed_len: section.compressed_len(),
                checksum: format!("0x{:04X}", section.checksum()),
                decoded_len: section.decoded().len(),
                decoded_sha256: section.decoded_sha256(),
            },
            Err(error) => Self::Invalid {
                reason: error.to_string(),
            },
        })
    }
}

pub fn inspect(args: InspectArgs) -> Result<()> {
    let report = if args.structure {
        inspect_structure(&args.input)?
    } else {
        let identity = identify_firmware(&args.input).with_context(|| {
            format!(
                "failed to inspect firmware identity for '{}'",
                args.input.display()
            )
        })?;
        InspectReport {
            identity,
            container: None,
            roundtrip_verified: None,
            application: None,
        }
    };

    match args.format {
        OutputFormat::Text => print_text(&report),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report)
                .context("failed to serialize inspection output as JSON")?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Reads the input once; identity and structure are derived from the same bytes.
fn inspect_structure(input: &Path) -> Result<InspectReport> {
    let (identity, bytes) = read_firmware(input)
        .with_context(|| format!("failed to read input firmware '{}'", input.display()))?;
    let container = parse_upd(&bytes)
        .with_context(|| format!("input '{}' is not a valid .UPD container", input.display()))?;
    // From here on the input is valid; any failure is a defect in patch-core, not in the file.
    container.verify_reproduces(&bytes).with_context(|| {
        format!(
            "internal error: re-serializing '{}' did not reproduce it byte-for-byte; \
             please report this as a patch-cli bug",
            input.display()
        )
    })?;
    let summary = container.summary().with_context(|| {
        format!(
            "internal error: failed to summarize '{}'; please report this as a patch-cli bug",
            input.display()
        )
    })?;
    Ok(InspectReport {
        identity,
        application: ApplicationReport::from_container(&container),
        container: Some(summary),
        roundtrip_verified: Some(true),
    })
}

fn print_text(report: &InspectReport) {
    println!("file_name: {}", report.identity.file_name);
    println!("size_bytes: {}", report.identity.size_bytes);
    println!("sha256_hex: {}", report.identity.sha256_hex);
    let Some(container) = &report.container else {
        return;
    };
    println!("documents: {}", container.documents.len());
    if report.roundtrip_verified == Some(true) {
        println!("roundtrip: byte-identical");
    }
    match &report.application {
        Some(ApplicationReport::Decoded {
            offset,
            compressed_len,
            checksum,
            decoded_len,
            decoded_sha256,
        }) => println!(
            "application: offset={offset} compressed_len={compressed_len} checksum={checksum} (ok) \
             decoded_len={decoded_len} decoded_sha256={decoded_sha256}"
        ),
        Some(ApplicationReport::Invalid { reason }) => println!("application: invalid ({reason})"),
        None => {}
    }
    for doc in &container.documents {
        let prefix = format!("document[{}]", doc.index);
        println!(
            "{prefix}: kind={} model={:?} version={:?} offset={} length={} crc16={} (ok)",
            doc.kind, doc.model, doc.version, doc.offset, doc.length, doc.crc16
        );
        println!(
            "{prefix}.records: data={} types={} bytes={} termination={} entry={}",
            doc.data_records,
            doc.data_record_types
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            doc.data_bytes,
            doc.termination_type,
            doc.entry_address
        );
        let extents: Vec<String> = doc
            .extents
            .iter()
            .map(|extent| format!("0x{:06X}..0x{:06X}", extent.start, extent.end))
            .collect();
        println!("{prefix}.extents: {}", extents.join(" "));
        match &doc.image {
            ImageReport::Reconstructed { base, len, sha256 } => {
                println!("{prefix}.image: base={base} len={len} sha256={sha256}");
            }
            ImageReport::SpanExceedsCap => println!(
                "{prefix}.image: not reconstructed (span {} bytes exceeds the {MAX_IMAGE_LEN}-byte cap)",
                doc.image_span
            ),
            ImageReport::BudgetExhausted => println!(
                "{prefix}.image: not reconstructed (span {} bytes; the {MAX_TOTAL_IMAGE_LEN}-byte \
                 total image budget is used up)",
                doc.image_span
            ),
        }
        println!(
            "{prefix}.descriptor: reserved_hex={} header=\"{}\"",
            doc.reserved_hex, doc.header_text
        );
    }
}
