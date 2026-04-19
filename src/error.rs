use serde_json::Value;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UsageError,
    NotFound,
    Conflict,
    ValidationError,
    StorageError,
    Interrupted,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UsageError => "usage_error",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::ValidationError => "validation_error",
            Self::StorageError => "storage_error",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Self::UsageError => 1,
            Self::NotFound => 2,
            Self::Conflict => 3,
            Self::ValidationError => 4,
            Self::StorageError => 5,
            Self::Interrupted => 130,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TkError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl TkError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UsageError, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::StorageError, message)
    }
}

impl Display for TkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TkError {}
