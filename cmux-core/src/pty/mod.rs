#[cfg(windows)]
pub mod conpty;

#[cfg(windows)]
pub use conpty::{ConPty, ConPtyConfig};
