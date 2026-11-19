//! I/O operations for astronomical image formats

pub mod fits;
pub mod xisf;

/// Bounded all-image inventory, exact UInt16 I/O, and capability assessment.
pub mod typed;

/// Read-only structural and payload validation for local image files.
pub mod validation;
