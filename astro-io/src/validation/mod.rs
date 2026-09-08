//! Read-only validation of local FITS and monolithic XISF containers.
//!
//! Structural success establishes declared layout completeness, not payload
//! integrity or producer completion. Full validation additionally reads all file
//! bytes, decodes supported payloads, and verifies present checksums. Neither
//! level proves that a writer will not modify the file after this call.
//!
//! ```no_run
//! use astro_io::validation::{validate_file, ValidationLevel, ValidationOptions};
//! use std::{path::Path, sync::atomic::AtomicBool};
//!
//! let options = ValidationOptions::default().with_level(ValidationLevel::Full);
//! let cancel = AtomicBool::new(false);
//! let report = validate_file(Path::new("capture.xisf"), &options, Some(&cancel))?;
//! assert_eq!(report.checksums().unchecked(), 0);
//! # Ok::<(), astro_io::validation::ValidationError>(())
//! ```
mod fits;
mod input;
mod resources;
mod xisf;
use resources::{Account, Buffer, Reservation};
pub use resources::{MemoryBudget, MemoryReservation};

use std::{
    fmt,
    fs::{File, Metadata},
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

/// Requested validation guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// Check declared layout and extents without decoding attached payloads.
    Structural,
    /// Also read the complete file, decode compression, and verify present checksums.
    Full,
}
/// Recognized container format, determined from bytes rather than the filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// A FITS container with the standard SIMPLE signature.
    Fits,
    /// A monolithic XISF 1.0 container.
    Xisf,
}
/// Distinct failure categories. Consumers decide retry and presentation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationErrorKind {
    /// Required bytes are absent from the observed file.
    Incomplete,
    /// Available layout bytes are malformed or contradictory.
    InvalidStructure,
    /// A checksum differs or a payload fails decoding.
    IntegrityMismatch,
    /// A required format feature cannot be checked at the requested level.
    Unsupported,
    /// A configured resource bound would be exceeded.
    ResourceLimit,
    /// Shared memory admission is temporarily occupied; retry outside the
    /// validator worker after other users release their reservations.
    ResourceBusy,
    /// The source observation or path identity changed during the call.
    ChangedDuringValidation,
    /// The caller requested cancellation.
    Cancelled,
    /// An operating system file access or read failed.
    Io,
}
/// A validation failure with context and, for I/O failures, the original cause.
#[derive(Debug)]
pub struct ValidationError {
    kind: ValidationErrorKind,
    message: String,
    source: Option<io::Error>,
}
impl ValidationError {
    /// Stable machine-readable classification.
    pub fn kind(&self) -> ValidationErrorKind {
        self.kind
    }
    fn new(kind: ValidationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }
    fn io(error: io::Error) -> Self {
        let kind = if error.kind() == io::ErrorKind::UnexpectedEof {
            ValidationErrorKind::Incomplete
        } else {
            ValidationErrorKind::Io
        };
        Self {
            kind,
            message: error.to_string(),
            source: Some(error),
        }
    }
    fn context(mut self, context: impl fmt::Display) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }
}
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e as _)
    }
}
type Result<T> = std::result::Result<T, ValidationError>;
fn invalid(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationErrorKind::InvalidStructure, message)
}
fn unsupported(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationErrorKind::Unsupported, message)
}
fn limit(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationErrorKind::ResourceLimit, message)
}
fn integrity(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationErrorKind::IntegrityMismatch, message)
}
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("byte count overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("dimension/byte count overflow"))
}

/// Validation resource limits. All builder methods reject zero values.
/// Working bytes cover live buffers plus conservative metadata/backend allowances.
/// See the README for parser/native allocation limitations and shared admission.
#[derive(Debug, Clone)]
pub struct ValidationLimits {
    header: u64,
    working: u64,
    structures: u64,
    decoded: u64,
}
impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            header: 64 << 20,
            working: 256 << 20,
            structures: 100_000,
            decoded: 64 << 30,
        }
    }
}
impl ValidationLimits {
    /// Maximum combined parsed header bytes.
    pub fn with_max_header_bytes(mut self, value: u64) -> Result<Self> {
        nonzero(value)?;
        self.header = value;
        Ok(self)
    }
    /// Maximum validator-managed live buffer budget.
    pub fn with_max_working_bytes(mut self, value: u64) -> Result<Self> {
        nonzero(value)?;
        self.working = value;
        Ok(self)
    }
    /// Maximum HDUs, cards, XML elements, or block descriptors.
    pub fn with_max_structures(mut self, value: u64) -> Result<Self> {
        nonzero(value)?;
        self.structures = value;
        Ok(self)
    }
    /// Maximum declared decoded bytes across data blocks.
    pub fn with_max_decoded_bytes(mut self, value: u64) -> Result<Self> {
        nonzero(value)?;
        self.decoded = value;
        Ok(self)
    }
    /// Configured combined header-byte limit.
    pub fn max_header_bytes(&self) -> u64 {
        self.header
    }
    /// Configured validator working-buffer budget.
    pub fn max_working_bytes(&self) -> u64 {
        self.working
    }
    /// Configured structure-count limit.
    pub fn max_structures(&self) -> u64 {
        self.structures
    }
    /// Configured combined decoded-byte limit.
    pub fn max_decoded_bytes(&self) -> u64 {
        self.decoded
    }
}
fn nonzero(value: u64) -> Result<()> {
    if value == 0 {
        Err(limit("validation limits must be nonzero"))
    } else {
        Ok(())
    }
}
/// Options are separate from application polling/retry policy.
#[derive(Debug, Clone)]
pub struct ValidationOptions {
    level: ValidationLevel,
    limits: ValidationLimits,
}
impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            level: ValidationLevel::Structural,
            limits: ValidationLimits::default(),
        }
    }
}
impl ValidationOptions {
    /// Select the requested guarantee; defaults to structural validation.
    pub fn with_level(mut self, level: ValidationLevel) -> Self {
        self.level = level;
        self
    }
    /// Replace the default resource limits.
    pub fn with_limits(mut self, limits: ValidationLimits) -> Self {
        self.limits = limits;
        self
    }
    /// The selected or successfully completed validation level.
    pub fn level(&self) -> ValidationLevel {
        self.level
    }
    /// The resource limits for this call.
    pub fn limits(&self) -> &ValidationLimits {
        &self.limits
    }
}
/// File observation, not an immutable snapshot. Identity is available on Unix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    size: u64,
    modified: Option<SystemTime>,
    identity: Option<(u64, u64)>,
}
impl FileStamp {
    fn from_metadata(m: &Metadata) -> Self {
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some((m.dev(), m.ino()))
        };
        #[cfg(not(unix))]
        let identity = None;
        Self {
            size: m.len(),
            modified: m.modified().ok(),
            identity,
        }
    }
    /// Observed physical file length in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
    /// Observed modification time, when supplied by the filesystem.
    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
    /// Unix device/inode pair; `None` on platforms without an implemented identity provider.
    pub fn identity(&self) -> Option<(u64, u64)> {
        self.identity
    }
}
/// Checksum counts. Structural mode leaves present checksums unchecked.
#[derive(Debug, Clone, Default)]
pub struct ChecksumSummary {
    present: u64,
    verified: u64,
}
impl ChecksumSummary {
    /// Number of declared checksums.
    pub fn present(&self) -> u64 {
        self.present
    }
    /// Number of checksums verified against stored bytes.
    pub fn verified(&self) -> u64 {
        self.verified
    }
    /// Number of declared checksums not verified (zero after full success).
    pub fn unchecked(&self) -> u64 {
        self.present - self.verified
    }
}
/// Completed checks. A report is returned only on success at the requested level.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    peak_reserved_bytes: u64,
    format: FileFormat,
    level: ValidationLevel,
    stamp: FileStamp,
    images: u64,
    structures: u64,
    bytes_read: u64,
    checksums: ChecksumSummary,
    undecoded_codecs: Vec<String>,
}
impl ValidationReport {
    /// Peak per-call reservations, including explicit parser/backend allowances.
    /// This is accounting telemetry, not a measurement of process RSS.
    pub fn peak_reserved_bytes(&self) -> u64 {
        self.peak_reserved_bytes
    }
    /// Format identified from the file signature.
    pub fn format(&self) -> FileFormat {
        self.format
    }
    /// The selected or successfully completed validation level.
    pub fn level(&self) -> ValidationLevel {
        self.level
    }
    /// Source observation used for validation; callers should recheck before use.
    pub fn stamp(&self) -> &FileStamp {
        &self.stamp
    }
    /// Number of physical image arrays; XISF aliases and thumbnails are not additional images.
    pub fn image_count(&self) -> u64 {
        self.images
    }
    /// Count of parsed header cards, XML elements, descriptors and decoded Zstandard frames.
    pub fn structure_count(&self) -> u64 {
        self.structures
    }
    /// Physical bytes read by validation I/O, including repeated reads.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
    /// Summary of optional checksum coverage.
    pub fn checksums(&self) -> &ChecksumSummary {
        &self.checksums
    }
    /// Unique compression algorithms whose payloads were not decoded.
    /// Empty after full validation; structural results include known and unknown codecs.
    pub fn undecoded_codecs(&self) -> &[String] {
        &self.undecoded_codecs
    }
}
struct Context<'a> {
    file: File,
    path: &'a Path,
    options: &'a ValidationOptions,
    cancel: Option<&'a AtomicBool>,
    stamp: FileStamp,
    bytes_read: u64,
    structures: u64,
    header_bytes: u64,
    memory: Account,
    undecoded_codecs: Vec<String>,
    decoded: u64,
    images: u64,
    checksums: ChecksumSummary,
    diagnostics: Reservation,
}
impl Context<'_> {
    fn checkpoint(&self) -> Result<()> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            Err(ValidationError::new(
                ValidationErrorKind::Cancelled,
                "validation cancelled",
            ))
        } else {
            Ok(())
        }
    }
    fn consistent(&self) -> Result<()> {
        let handle = self.file.metadata().map_err(ValidationError::io)?;
        let path = std::fs::metadata(self.path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                ValidationError::new(
                    ValidationErrorKind::ChangedDuringValidation,
                    "source path disappeared",
                )
            } else {
                ValidationError::io(e)
            }
        })?;
        if FileStamp::from_metadata(&handle) != self.stamp
            || FileStamp::from_metadata(&path) != self.stamp
        {
            Err(ValidationError::new(
                ValidationErrorKind::ChangedDuringValidation,
                "source changed during validation",
            ))
        } else {
            Ok(())
        }
    }
    fn extent(&self, offset: u64, length: u64) -> Result<()> {
        if add(offset, length)? > self.stamp.size {
            Err(ValidationError::new(
                ValidationErrorKind::Incomplete,
                format!(
                    "data range {offset}+{length} exceeds file size {}",
                    self.stamp.size
                ),
            ))
        } else {
            Ok(())
        }
    }
    fn undecoded(&mut self, codec: &str) -> Result<()> {
        if !self.undecoded_codecs.iter().any(|s| s == codec) {
            let bytes = add(codec.len() as u64, 128)?;
            self.diagnostics.grow(bytes)?;
            self.undecoded_codecs
                .try_reserve(1)
                .map_err(|_| limit("codec diagnostic allocation failed"))?;
            self.undecoded_codecs.push(codec.to_string());
        }
        Ok(())
    }
    fn structure(&mut self) -> Result<()> {
        self.structures = add(self.structures, 1)?;
        if self.structures > self.options.limits.structures {
            Err(limit("structure count limit exceeded"))
        } else {
            Ok(())
        }
    }
    fn header(&mut self, bytes: u64) -> Result<()> {
        self.header_bytes = add(self.header_bytes, bytes)?;
        if self.header_bytes > self.options.limits.header {
            Err(limit("header byte limit exceeded"))
        } else {
            Ok(())
        }
    }
    fn decoded(&mut self, bytes: u64) -> Result<()> {
        self.decoded = add(self.decoded, bytes)?;
        if self.decoded > self.options.limits.decoded {
            Err(limit("decoded byte limit exceeded"))
        } else {
            Ok(())
        }
    }
    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        self.extent(offset, buffer.len() as u64)?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(ValidationError::io)?;
        for chunk in buffer.chunks_mut(64 * 1024) {
            self.checkpoint()?;
            self.file.read_exact(chunk).map_err(ValidationError::io)?;
            self.bytes_read = add(self.bytes_read, chunk.len() as u64)?;
        }
        Ok(())
    }
    fn bytes(&mut self, offset: u64, size: u64) -> Result<Buffer> {
        self.extent(offset, size)?;
        let len = usize::try_from(size).map_err(|_| limit("buffer exceeds address space"))?;
        let mut b = self.memory.buffer(len)?;
        self.read(offset, &mut b)?;
        Ok(b)
    }
    fn stream(&mut self, offset: u64, size: u64, mut consume: impl FnMut(&[u8])) -> Result<()> {
        self.extent(offset, size)?;
        if size == 0 {
            return Ok(());
        }
        let available = self.memory.remaining();
        if available == 0 {
            return Err(limit("no working memory available for I/O"));
        }
        let cap = size.min(64 * 1024).min(available) as usize;
        let mut b = self.memory.buffer(cap)?;
        let mut done = 0;
        while done < size {
            let n = (size - done).min(cap as u64) as usize;
            self.read(add(offset, done)?, &mut b[..n])?;
            consume(&b[..n]);
            done += n as u64;
        }
        Ok(())
    }
}
/// Validate a local file without modifying it.
///
/// The extension is ignored. Full mode may read the file more than once.
/// Cancellation is cooperative between chunks; blocking OS/codec calls may
/// delay it. Missing optional checksums are allowed. Errors distinguish incomplete,
/// invalid, unsupported, changed, cancelled, resource-limited, and unreadable input.
pub fn validate_file(
    path: &Path,
    options: &ValidationOptions,
    cancel: Option<&AtomicBool>,
) -> Result<ValidationReport> {
    let budget = MemoryBudget::new(options.limits.working)?;
    validate_controlled(path, options, cancel, &budget)
}
/// Validate using a caller-owned shared memory allowance, without waiting.
///
/// Share one budget across concurrent calls. `ResourceBusy` is temporary capacity
/// contention; it is not file corruption. Every failure releases this call's
/// reservations before returning. Callers own queuing, fairness and cancellation.
/// FITS GZIP, Rice (1/2/4-byte), PLIO and HCOMPRESS use managed decoding;
/// quantized/fallback/mask layouts are checked without a native allocation path.
/// GZIP/Rice/PLIO stream; HCOMPRESS admits a complete bounded tile working set.
/// Unsupported extensions return an error without weaker/native fallback.
/// Quantization metadata and payload integrity are checked, not rendered pixels
/// or the scientific fidelity of lossy compression. Supported XISF codecs retain
/// their documented streaming or admitted whole-block resource requirements.
/// Returned reports and caller-owned queues are outside the working allowance.
///
/// ```no_run
/// use astro_io::validation::{MemoryBudget, ValidationOptions, validate_file_with_budget};
/// use std::path::Path;
/// let budget = MemoryBudget::new(64 * 1024 * 1024)?;
/// // Clone/share this same budget across concurrent calls; do not make one per file.
/// let report = validate_file_with_budget(Path::new("image.xisf"),
///     &ValidationOptions::default(), None, &budget)?;
/// assert_eq!(budget.used_bytes(), 0);
/// # Ok::<(), astro_io::validation::ValidationError>(())
/// ```
pub fn validate_file_with_budget(
    path: &Path,
    options: &ValidationOptions,
    cancel: Option<&AtomicBool>,
    budget: &MemoryBudget,
) -> Result<ValidationReport> {
    validate_controlled(path, options, cancel, budget)
}
fn validate_controlled(
    path: &Path,
    options: &ValidationOptions,
    cancel: Option<&AtomicBool>,
    budget: &MemoryBudget,
) -> Result<ValidationReport> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err(ValidationError::new(
            ValidationErrorKind::Cancelled,
            "validation cancelled",
        ));
    }
    let file = File::open(path).map_err(ValidationError::io)?;
    let metadata = file.metadata().map_err(ValidationError::io)?;
    if !metadata.is_file() {
        return Err(unsupported("validation requires a regular file"));
    }
    let stamp = FileStamp::from_metadata(&metadata);
    let memory = Account::new(options.limits.working, budget)?;
    let diagnostics = memory.reserve(0)?;
    let mut c = Context {
        file,
        path,
        options,
        cancel,
        stamp,
        bytes_read: 0,
        structures: 0,
        header_bytes: 0,
        memory,
        diagnostics,
        undecoded_codecs: Vec::new(),
        decoded: 0,
        images: 0,
        checksums: ChecksumSummary::default(),
    };
    let result = (|| {
        let mut signature = [0; 8];
        c.read(0, &mut signature)?;
        let format = if &signature == b"XISF0100" {
            xisf::validate(&mut c)?;
            FileFormat::Xisf
        } else if &signature == b"SIMPLE  " {
            fits::validate(&mut c)?;
            FileFormat::Fits
        } else {
            return Err(unsupported(
                "unrecognized file signature (expected FITS or monolithic XISF 1.0)",
            ));
        };
        if options.level == ValidationLevel::Full {
            c.stream(0, c.stamp.size, |_| {})?;
        }
        c.checkpoint()?;
        Ok(format)
    })();
    // A concurrent source change invalidates both success and format findings.
    // Preserve cancellation as the terminal outcome requested by the caller.
    if !matches!(&result,Err(e) if e.kind()==ValidationErrorKind::Cancelled) {
        c.consistent()?;
    }
    let format = result.map_err(|e| e.context(path.display()))?;
    Ok(ValidationReport {
        peak_reserved_bytes: c.memory.peak(),
        format,
        level: options.level,
        stamp: c.stamp,
        images: c.images,
        structures: c.structures,
        bytes_read: c.bytes_read,
        checksums: c.checksums,
        undecoded_codecs: c.undecoded_codecs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn with_context(test: impl FnOnce(&mut Context<'_>, &AtomicBool)) {
        let path = std::env::temp_dir().join(format!(
            "astro-validation-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, vec![0; 150_000]).unwrap();
        let file = File::open(&path).unwrap();
        let stamp = FileStamp::from_metadata(&file.metadata().unwrap());
        let options = ValidationOptions::default();
        let cancel = AtomicBool::new(false);
        let budget = MemoryBudget::new(options.limits.working).unwrap();
        let memory = Account::new(options.limits.working, &budget).unwrap();
        let diagnostics = memory.reserve(0).unwrap();
        let mut c = Context {
            file,
            path: &path,
            options: &options,
            cancel: Some(&cancel),
            stamp,
            bytes_read: 0,
            structures: 0,
            header_bytes: 0,
            memory,
            diagnostics,
            undecoded_codecs: Vec::new(),
            decoded: 0,
            images: 0,
            checksums: ChecksumSummary::default(),
        };
        test(&mut c, &cancel);
        drop(c);
        assert_eq!(budget.used_bytes(), 0);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn cancellation_between_read_chunks_does_not_read_the_rest() {
        with_context(|c, cancel| {
            let error = c
                .stream(0, c.stamp.size, |_| cancel.store(true, Ordering::Relaxed))
                .unwrap_err();
            assert_eq!(error.kind(), ValidationErrorKind::Cancelled);
            assert_eq!(c.bytes_read, 65536);
            assert_eq!(c.memory.remaining(), c.options.limits.working);
        });
    }
    #[test]
    fn payload_input_checks_cancellation_even_with_buffered_bytes() {
        use std::io::BufRead;
        with_context(|c, cancel| {
            {
                let mut input = input::Input::attached(c, 0, 100_000).unwrap();
                assert_eq!(input.fill_buf().unwrap().len(), 65536);
                input.consume(1);
                cancel.store(true, Ordering::Relaxed);
                let error = input::decode_error(input.fill_buf().unwrap_err());
                assert_eq!(error.kind(), ValidationErrorKind::Cancelled);
            }
            assert_eq!(c.bytes_read, 65536);
            assert_eq!(c.memory.remaining(), c.options.limits.working);
        });
    }
    #[test]
    fn mutation_during_a_read_invalidates_the_observation() {
        with_context(|c, _| {
            let path = c.path;
            c.stream(0, c.stamp.size, |_| {
                let writer = std::fs::OpenOptions::new().write(true).open(path).unwrap();
                writer.set_len(160_000).unwrap();
            })
            .unwrap();
            assert_eq!(
                c.consistent().unwrap_err().kind(),
                ValidationErrorKind::ChangedDuringValidation
            );
        });
    }
    #[cfg(unix)]
    #[test]
    fn path_replacement_with_matching_length_and_mtime_is_detected() {
        with_context(|c, _| {
            std::fs::remove_file(c.path).unwrap();
            std::fs::write(c.path, vec![0; c.stamp.size as usize]).unwrap();
            File::open(c.path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(c.stamp.modified.unwrap()))
                .unwrap();
            assert_eq!(
                c.consistent().unwrap_err().kind(),
                ValidationErrorKind::ChangedDuringValidation
            );
        });
    }
    #[test]
    fn disappearance_is_a_changed_source() {
        with_context(|c, _| {
            std::fs::remove_file(c.path).unwrap();
            assert_eq!(
                c.consistent().unwrap_err().kind(),
                ValidationErrorKind::ChangedDuringValidation
            );
        });
    }
}
