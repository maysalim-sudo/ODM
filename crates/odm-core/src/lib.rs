//! Shared pure types and helpers for ODM. No I/O lives here.
pub mod ids;
pub mod state;
pub mod error;
pub mod filename;
pub mod category;

pub use state::DownloadState;
pub use error::{ErrorKind, OdmError};
pub use category::Category;
