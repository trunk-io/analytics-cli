#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorType {
    Client,
    Server,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    status_code: u16,
    error_type: ErrorType,
}

impl Error {
    pub fn new(message: String, status_code: u16, error_type: ErrorType) -> Self {
        Self {
            message,
            status_code,
            error_type,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: {}", self.message)
    }
}
