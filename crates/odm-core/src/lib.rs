//! Shared pure types and helpers for ODM. No I/O lives here.
pub mod ids;
pub mod state;
pub mod error;

pub use state::DownloadState;
pub use error::{ErrorKind, OdmError};
