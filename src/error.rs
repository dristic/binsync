use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("File not found {0}")]
    FileNotFound(PathBuf),

    #[error("Directory not found {0}")]
    DirectoryNotFound(PathBuf),

    #[error("Chunk not found {0}")]
    ChunkNotFound(u64),

    #[error("Access is denied")]
    AccessDenied,

    #[error("Unspecified: {0}")]
    Unspecified(String),

    #[error(transparent)]
    IOError(#[from] std::io::Error),
}
