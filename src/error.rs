use std::{error, fmt};

#[derive(Debug)]
pub enum Error {
    Default,
    FromFileNotFound,
}

impl Error {
    pub fn new(msg: &str) -> Error {
        Error::Default
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Default => write!(f, "Default"),
            Error::FromFileNotFound => write!(f, "From file not found"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match self {
            Error::Default => "An undefined error has occurred.",
            Error::FromFileNotFound => "The from folder parameter was not found.",
        }
    }
}
