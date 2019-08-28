use std::error;
use std::fmt;

#[derive(Debug)]
pub enum UMassBotError {
    RequestError(reqwest::Error),
    IoError(std::io::Error),
    SerenityError(serenity::Error),
}

pub type Result<T> = std::result::Result<T, UMassBotError>;

impl fmt::Display for UMassBotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &*self {
            UMassBotError::RequestError(ref err) => write!(f, "Request error: {}", err),
            UMassBotError::IoError(ref err) => write!(f, "IO error: {}", err),
            UMassBotError::SerenityError(ref err) => write!(f, "Discord error: {}", err),
        }
    }
}

impl error::Error for UMassBotError {
    fn description(&self) -> &str {
        match *self {
            UMassBotError::RequestError(ref err) => err.description(),
            UMassBotError::IoError(ref err) => err.description(),
            UMassBotError::SerenityError(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            UMassBotError::RequestError(ref err) => Some(err),
            UMassBotError::IoError(ref err) => Some(err),
            UMassBotError::SerenityError(ref err) => Some(err),
        }
    }
}

impl From<serenity::Error> for UMassBotError {
    fn from(err: serenity::Error) -> UMassBotError {
        UMassBotError::SerenityError(err)
    }
}

impl From<reqwest::Error> for UMassBotError {
    fn from(err: reqwest::Error) -> UMassBotError {
        UMassBotError::RequestError(err)
    }
}

impl From<std::io::Error> for UMassBotError {
    fn from(err: std::io::Error) -> UMassBotError {
        UMassBotError::IoError(err)
    }
}
