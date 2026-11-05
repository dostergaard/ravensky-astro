//! I/O operations for astronomical image formats

pub mod fits;
pub mod xisf;

/// Bounded, all-image source inventory for later conversion policy and typed I/O.
pub mod typed;

/// Read-only structural and payload validation for local image files.
pub mod validation;
