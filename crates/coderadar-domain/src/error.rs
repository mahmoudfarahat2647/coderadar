use std::fmt;

/// Domain-level error type.
///
/// Lives in the innermost circle and therefore only uses `std`. Outer layers
/// map their driver-specific errors (I/O, HTTP, SQL) into this type at the
/// boundary so that the domain and use cases never learn about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// A domain invariant was violated (e.g. a score outside `[0.0, 1.0]`).
    Invariant(String),
    /// A value could not be parsed into a domain type.
    Parse(String),
    /// An I/O operation failed (mapped from `std::io::Error` by adapters).
    Io(String),
    /// An external agency (Jev, database, ...) failed.
    External(String),
    /// An operation was refused by an authority check (signature, policy).
    Permission(String),
    /// A requested entity was not found.
    NotFound(String),
}

impl DomainError {
    /// Build an [`DomainError::Invariant`] from any string-like value.
    pub fn invariant(message: impl Into<String>) -> Self {
        Self::Invariant(message.into())
    }

    /// Build a [`DomainError::Parse`] from any string-like value.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse(message.into())
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invariant(message) => write!(f, "domain invariant violated: {message}"),
            Self::Parse(message) => write!(f, "failed to parse domain value: {message}"),
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::External(message) => write!(f, "external service error: {message}"),
            Self::Permission(message) => write!(f, "permission refused: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
        }
    }
}

impl std::error::Error for DomainError {}

impl From<std::io::Error> for DomainError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
