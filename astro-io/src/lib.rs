//! I/O operations for astronomical image formats

pub mod fits;
pub mod xisf;

/// Read-only structural and payload validation for local image files.
pub mod validation;
