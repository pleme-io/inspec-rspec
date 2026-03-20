use std::path::PathBuf;

/// Errors that can occur during InSpec-to-RSpec transpilation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to parse an InSpec control file.
    #[error("parse error in {file}:{line}: {message}")]
    Parse {
        file: String,
        line: usize,
        message: String,
    },

    /// Failed to transpile a control to RSpec.
    #[error("transpile error for control '{control_id}': {message}")]
    Transpile {
        control_id: String,
        message: String,
    },

    /// I/O error reading or writing files.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The specified path is not a valid InSpec profile directory.
    #[error("not a valid InSpec profile directory: {path}")]
    InvalidProfile { path: PathBuf },

    /// No controls found in the profile.
    #[error("no controls found in profile directory: {path}")]
    NoControls { path: PathBuf },
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, Error>;
