use patch_schema::SchemaValidationError;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatchCoreError {
    #[error("input path does not point to a regular file: {path}")]
    InputNotAFile { path: String },

    #[error("failed to read firmware file '{path}': {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Refusal reasons from [`crate::apply_recipe`]. Every variant means no output may be written.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchEngineError {
    #[error("recipe manifest is invalid: {0}")]
    InvalidManifest(#[from] SchemaValidationError),

    #[error(
        "input firmware (size={size_bytes}, sha256={sha256_hex}) is not compatible with recipe '{recipe_id}'"
    )]
    IncompatibleInput {
        recipe_id: String,
        size_bytes: u64,
        sha256_hex: String,
    },

    #[error(
        "operations[{operation_index}] {region}: out of bounds (offset={offset}, length={length}, input_size={input_size})"
    )]
    RangeOutOfBounds {
        operation_index: usize,
        region: OperationRegion,
        offset: u64,
        length: u64,
        input_size: usize,
    },

    #[error(
        "operations[{operation_index}] destination range {start}..{end} overlaps with operations[{other_operation_index}] range {other_start}..{other_end}"
    )]
    OverlappingDestinations {
        operation_index: usize,
        start: usize,
        end: usize,
        other_operation_index: usize,
        other_start: usize,
        other_end: usize,
    },

    #[error(
        "mutation verification failed: input and output lengths differ (input={input_len}, output={output_len})"
    )]
    OutputLengthChanged { input_len: usize, output_len: usize },

    #[error(
        "mutation verification failed: byte offset {byte_offset} changed outside declared destination ranges"
    )]
    MutationOutsideDeclaredRegions { byte_offset: usize },

    #[error(
        "output SHA-256 mismatch for recipe '{recipe_id}': expected={expected}, actual={actual}"
    )]
    OutputHashMismatch {
        recipe_id: String,
        expected: String,
        actual: String,
    },
}

/// Which range of an operation failed a bounds check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationRegion {
    WriteSpanDestination,
    OwnerCopySource,
    OwnerCopyDestination,
}

impl fmt::Display for OperationRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::WriteSpanDestination => "write_span destination",
            Self::OwnerCopySource => "owner_copy_window source",
            Self::OwnerCopyDestination => "owner_copy_window destination",
        };
        f.write_str(label)
    }
}

/// Structural violations found by [`crate::upd::parse_upd`].
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum UpdError {
    #[error("container does not start with a decimal document-length header")]
    MissingLengthHeader,

    #[error("container document-length header is malformed")]
    MalformedLengthHeader,

    #[error(
        "container document lengths sum to {declared} bytes but {available} bytes follow the header"
    )]
    LengthHeaderMismatch { declared: u64, available: usize },

    #[error("document[{document_index}] is too short ({length} bytes)")]
    DocumentTooShort {
        document_index: usize,
        length: usize,
    },

    #[error(
        "document[{document_index}] CRC-16 mismatch: stored=0x{stored:04X}, computed=0x{computed:04X}"
    )]
    DocumentCrcMismatch {
        document_index: usize,
        stored: u16,
        computed: u16,
    },

    #[error("document[{document_index}] descriptor field '{field}' is invalid")]
    InvalidDescriptor {
        document_index: usize,
        field: &'static str,
    },

    #[error("document[{document_index}] S-record body does not end with CRLF")]
    MissingFinalCrlf { document_index: usize },

    #[error("document[{document_index}] record[{record_index}]: {defect}")]
    MalformedRecord {
        document_index: usize,
        record_index: usize,
        defect: RecordDefect,
    },

    #[error("document[{document_index}] record[{record_index}]: {violation}")]
    UnexpectedRecordLayout {
        document_index: usize,
        record_index: usize,
        violation: LayoutViolation,
    },

    #[error("serializer self-check failed: output does not re-parse to the same container")]
    SerializerSelfCheckFailed,

    #[error("serializer self-check failed: output does not parse: {0}")]
    SerializerOutputUnparseable(Box<UpdError>),

    #[error("round trip is not byte-identical: first difference at byte offset {offset}")]
    RoundTripMismatch { offset: usize },

    #[error("document[{document_index}] image span of {span} bytes exceeds the supported maximum")]
    ImageTooLarge { document_index: usize, span: u64 },
}

/// Why a single S-record line was rejected.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum RecordDefect {
    #[error("not a CRLF-terminated uppercase-hex S-record line")]
    Syntax,
    #[error("unsupported S-record type")]
    UnsupportedType,
    #[error("byte count does not match record length")]
    ByteCountMismatch,
    #[error("record is shorter than its address field plus checksum")]
    TooShort,
    #[error("record checksum mismatch")]
    ChecksumMismatch,
    #[error("record data exceeds the address space of its type")]
    AddressOverflow,
}

/// Which S-record layout rule a document broke.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum LayoutViolation {
    #[error("first record must be an S0 header")]
    HeaderNotFirst,
    #[error("last record must be S7, S8, or S9")]
    TerminationNotLast,
    #[error("termination record must not carry data")]
    TerminationHasData,
    #[error("document contains no data records")]
    NoDataRecords,
    #[error("only S1/S2/S3 data records may appear between header and termination")]
    NonDataRecordInBody,
    #[error("data record carries no data")]
    EmptyDataRecord,
    #[error("data records must ascend without overlap")]
    DataNotAscending,
}

/// Why an LZSS stream could not be decoded.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum LzssError {
    #[error("LZSS stream ends inside a match token at input offset {input_offset}")]
    TruncatedMatch { input_offset: usize },

    #[error("LZSS output would exceed the {limit}-byte limit")]
    OutputLimitExceeded { limit: usize },

    #[error("section data must start with the 19 zero bytes the section tag decodes to")]
    MissingSectionSeed,

    #[error("LZSS encoder input of {len} bytes is too large")]
    InputTooLarge { len: usize },

    #[error("LZSS encoder self-check failed at data offset {position}")]
    EncoderSelfCheck { position: usize },
}

/// Why a compressed firmware section was refused.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SectionError {
    #[error("container has no XDJ-700 MAIN document")]
    NoMainDocument,

    #[error("container has {count} XDJ-700 MAIN documents; exactly one is required")]
    AmbiguousMainDocument { count: usize },

    #[error("MAIN version {version} has no verified application-section layout")]
    UnverifiedVersion { version: String },

    #[error("MAIN image could not be reconstructed: {0}")]
    Image(#[from] UpdError),

    #[error("MAIN image starts at {base:#X}; the application layout requires address 0")]
    ImageBase { base: u64 },

    #[error(
        "MAIN image was not reconstructed (over the per-document cap or the total image budget; \
         see its image status)"
    )]
    ImageNotReconstructed,

    #[error("section size field at offset {offset:#X} is outside the image")]
    SizeFieldOutOfBounds { offset: usize },

    #[error("section at offset {offset:#X} declares {declared_len} bytes, beyond the image end")]
    DataOutOfBounds { offset: usize, declared_len: u32 },

    #[error("section checksum after offset {offset:#X} is outside the image")]
    ChecksumOutOfBounds { offset: usize },

    #[error("section checksum mismatch: stored={stored:#06X}, computed={computed:#06X}")]
    ChecksumMismatch { stored: u16, computed: u16 },

    #[error("section stream does not start with the zero-prefix tag (literal 00, match EE FF)")]
    MissingTag,

    #[error("section stream could not be decoded: {0}")]
    Decode(#[from] LzssError),

    #[error("section could not be encoded: {0}")]
    Encode(LzssError),

    #[error("encoded section stream of {len} bytes does not fit the u32 size field")]
    SectionTooLarge { len: usize },

    #[error("encoded section failed its self-check: it does not decode: {0}")]
    EncodeSelfCheckDecode(Box<SectionError>),

    #[error("encoded section failed its self-check: it decodes to different bytes")]
    EncodeSelfCheckMismatch,
}
