//! Expected failures and how each is answered: a bad request (400), a permission refusal (403),
//! a workflow conflict with its own status, a database conflict (409) or busy database (503),
//! and anything unexpected (500).

use std::fmt;

use crate::sqlite;

#[derive(Debug)]
pub enum AppError {
    /// A refusal with its HTTP status and message.
    Status(u16, String),
    Db(sqlite::Error),
    Internal(String),
}

pub type R<T> = Result<T, AppError>;

/// Python's ValueError: the request is wrong (400).
pub fn bad(message: impl Into<String>) -> AppError {
    AppError::Status(400, message.into())
}

/// Python's PermissionError (403).
pub fn forbidden(message: impl Into<String>) -> AppError {
    AppError::Status(403, message.into())
}

/// A WorkflowError: a conflict by default (409), or the status given.
pub fn workflow(message: impl Into<String>) -> AppError {
    AppError::Status(409, message.into())
}

pub fn status(code: u16, message: impl Into<String>) -> AppError {
    AppError::Status(code, message.into())
}

pub fn internal(message: impl Into<String>) -> AppError {
    AppError::Internal(message.into())
}

impl From<sqlite::Error> for AppError {
    fn from(error: sqlite::Error) -> Self {
        AppError::Db(error)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::Internal(error.to_string())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Status(code, message) => write!(f, "{code}: {message}"),
            AppError::Db(error) => write!(f, "database: {error}"),
            AppError::Internal(message) => write!(f, "internal: {message}"),
        }
    }
}

/// Raise a ValueError-style refusal from an int()/float() conversion message.
pub trait OrBad<T> {
    fn or_bad(self) -> R<T>;
}

impl<T> OrBad<T> for Result<T, String> {
    fn or_bad(self) -> R<T> {
        self.map_err(bad)
    }
}

pub fn log_error(message: &str) {
    eprintln!("{} ERROR {message}", crate::clock::local_now().date_time_text());
}

pub fn log_warning(message: &str) {
    eprintln!("{} WARNING {message}", crate::clock::local_now().date_time_text());
}
