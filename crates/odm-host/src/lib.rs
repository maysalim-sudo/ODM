//! ODM native messaging host: relays browser frames to the ODM pipe.
pub mod relay;

pub use relay::{run, HostConfig};
pub mod register;
