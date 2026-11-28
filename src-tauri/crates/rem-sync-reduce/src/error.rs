use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ReduceError {
    InvalidInput(String),
    Json(serde_json::Error),
}

impl fmt::Display for ReduceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "{message}"),
            Self::Json(error) => write!(formatter, "invalid JSON: {error}"),
        }
    }
}

impl Error for ReduceError {}

impl From<serde_json::Error> for ReduceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
