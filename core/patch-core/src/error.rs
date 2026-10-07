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
