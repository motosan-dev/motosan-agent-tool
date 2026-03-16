/// Errors returned by tool operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Other(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Parse(err.to_string())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Other(_)));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn from_string() {
        let err: Error = "something broke".to_string().into();
        assert!(matches!(err, Error::Other(_)));
        assert_eq!(err.to_string(), "something broke");
    }

    #[test]
    fn from_str() {
        let err: Error = "oops".into();
        assert_eq!(err.to_string(), "oops");
    }

    #[test]
    fn error_new_still_works() {
        let err = Error::new("custom message");
        assert_eq!(err.to_string(), "custom message");
    }

    fn _demo_question_mark_io() -> Result<()> {
        let _ = std::fs::read_to_string("/nonexistent")?;
        Ok(())
    }

    fn _demo_question_mark_json() -> Result<serde_json::Value> {
        let v = serde_json::from_str("invalid")?;
        Ok(v)
    }
}
