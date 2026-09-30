//! Shared pure types and helpers for ODM. No I/O lives here.
pub mod category;
pub mod error;
pub mod filename;
pub mod ids;
pub mod state;

pub use category::Category;
pub use error::{ErrorKind, OdmError};
pub use state::DownloadState;
