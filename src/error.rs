use std::{error, fmt, path::PathBuf};

#[derive(Debug)]
pub enum Error {
    Default,
    FileNotFound(PathBuf),
    DirectoryNotFound(PathBuf),
    AccessDenied,
}

impl Error {
    pub fn new() -> Error {
        Error::Default
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Default => write!(f, "Default"),
            Error::FileNotFound(p) => write!(f, "File not found {:?}", p),
            Error::DirectoryNotFound(p) => write!(f, "Directory not found {:?}", p),
            Error::AccessDenied => write!(f, "Access denied during write operation"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match self {
            Error::Default => "An undefined error has occurred.",
            Error::FileNotFound(_) => "A given file was not found.",
            Error::DirectoryNotFound(_) => "A given directory was not found.",
            Error::AccessDenied => "A write or create operation failed.",
        }
    }
}
