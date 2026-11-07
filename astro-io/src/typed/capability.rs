//! Truthful initial format/sample/codec capability description for P2/P3.

use super::write::{
    fits_output_bound_u16, xisf_output_bound_u16, EncodedSizeBound, FitsCodec, U16ImageSpec,
    XisfCodec,
};
use super::ContainerFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleType {
    UInt16,
    Float32,
    Float64,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressionCodec {
    Uncompressed,
    Rice1,
    Gzip1,
    Gzip2,
    Zstd,
    Zlib,
    Lz4,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityQuery {
    pub format: ContainerFormat,
    pub sample_type: SampleType,
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub codec: CompressionCodec,
    pub byte_shuffle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cfitsio,
    RustXisf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    Supported {
        exact_reader: bool,
        exact_writer: bool,
        lossless: bool,
        backend: Backend,
        output_bound: EncodedSizeBound,
        /// Whole-image decoded-byte ceiling, if the reader requires one.
        /// Compressed XISF is spooled to disk and instead enforces per-subblock
        /// limits through `PixelReadLimits` and caller-owned scratch admission.
        max_decoded_read_bytes: Option<u64>,
    },
    Unsupported {
        reason: &'static str,
    },
}

impl Capability {
    /// Whether this bare-image writer configuration has a proven output bound.
    /// Metadata and scratch requirements must also be bounded by the caller.
    pub fn has_bounded_output(&self) -> bool {
        matches!(
            self,
            Self::Supported {
                output_bound: EncodedSizeBound::Finite(_),
                ..
            }
        )
    }
}

/// Describe the proven P1 subset for one monochrome image with no auxiliary
/// metadata. Multi-image output and additional property bounds must be
/// calculated with the corresponding writer bound APIs.
pub fn assess(query: &CapabilityQuery) -> Capability {
    use Capability::{Supported, Unsupported};
    if query.sample_type != SampleType::UInt16 {
        return Unsupported { reason: "P1 exact writer/reader supports UInt16 only; floating FITS tiled compression is not proven bitwise lossless" };
    }
    if query.width == 0 || query.height == 0 || query.channels != 1 {
        return Unsupported {
            reason: "P1 writer requires nonzero two-dimensional monochrome geometry",
        };
    }
    let image = U16ImageSpec {
        id: None,
        width: query.width,
        height: query.height,
    };
    let Some(_decoded) = query
        .width
        .checked_mul(query.height)
        .and_then(|count| count.checked_mul(2))
    else {
        return Unsupported {
            reason: "Image byte count overflows",
        };
    };
    match query.format {
        ContainerFormat::Fits => {
            if query.byte_shuffle {
                return Unsupported {
                    reason: "FITS byte shuffle is not a P1 codec setting",
                };
            }
            let codec = match &query.codec {
                CompressionCodec::Uncompressed => FitsCodec::Uncompressed,
                CompressionCodec::Rice1 => FitsCodec::Rice,
                CompressionCodec::Gzip1 => FitsCodec::Gzip1,
                CompressionCodec::Gzip2 => FitsCodec::Gzip2,
                _ => {
                    return Unsupported {
                        reason: "FITS codec is outside the proven UInt16 P1 subset",
                    }
                }
            };
            let bound = match fits_output_bound_u16(&[image], codec) {
                Ok(bound) => bound,
                Err(_) => {
                    return Unsupported {
                        reason: "FITS writer geometry is invalid",
                    }
                }
            };
            Supported {
                exact_reader: true,
                exact_writer: true,
                lossless: true,
                backend: Backend::Cfitsio,
                output_bound: bound,
                max_decoded_read_bytes: None,
            }
        }
        ContainerFormat::Xisf => {
            let codec = match &query.codec {
                CompressionCodec::Uncompressed => XisfCodec::Uncompressed,
                CompressionCodec::Zstd => XisfCodec::Zstd,
                CompressionCodec::Zlib => XisfCodec::Zlib,
                CompressionCodec::Lz4 => XisfCodec::Lz4,
                _ => {
                    return Unsupported {
                        reason: "XISF codec is outside the proven UInt16 P1 subset",
                    }
                }
            };
            if query.byte_shuffle && codec == XisfCodec::Uncompressed {
                return Unsupported {
                    reason: "XISF byte shuffle requires compression",
                };
            }
            let bound = match xisf_output_bound_u16(&[image], codec, query.byte_shuffle) {
                Ok(bound) => bound,
                Err(_) => {
                    return Unsupported {
                        reason: "XISF writer geometry or descriptor limit is exceeded",
                    }
                }
            };
            Supported {
                exact_reader: true,
                exact_writer: true,
                lossless: true,
                backend: Backend::RustXisf,
                output_bound: EncodedSizeBound::Finite(bound),
                max_decoded_read_bytes: None,
            }
        }
    }
}
