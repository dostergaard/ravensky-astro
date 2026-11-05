//! Reproducible P1 UInt16 writer/reader probe: 1024x1024 monochrome patterns.
//! Run with `cargo run -p astro-bench --example p1_typed_io [--smooth]` and record the
//! hardware, build profile, and storage conditions alongside output.

use anyhow::{Context, Result};
use astro_io::typed::write::{write_fits_u16, write_xisf_u16, FitsCodec, U16ImageSpec, XisfCodec};
use astro_io::typed::{inventory, InventoryLimits, PixelReadLimits, U16ChunkReader};
use std::{
    fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const WIDTH: usize = 1024;
const HEIGHT: usize = 1024;

fn pixel(index: usize, smooth: bool) -> u16 {
    let x = index % WIDTH;
    let y = index / WIDTH;
    if smooth {
        (((x / 16) * 37 + (y / 16) * 131) & 0xffff) as u16
    } else {
        ((x * 37 + y * 131) & 0xffff) as u16
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("astro-p1-typed-{}-{now}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn main() -> Result<()> {
    let smooth = std::env::args().any(|arg| arg == "--smooth");
    println!("pattern={}", if smooth { "smooth" } else { "gradient" });
    let scratch = Scratch::new()?;
    let images = [U16ImageSpec {
        id: Some("LIGHT"),
        width: WIDTH,
        height: HEIGHT,
    }];
    for (name, fits, xisf) in [
        ("fits-none", Some(FitsCodec::Uncompressed), None),
        ("fits-rice", Some(FitsCodec::Rice), None),
        ("fits-gzip1", Some(FitsCodec::Gzip1), None),
        ("fits-gzip2", Some(FitsCodec::Gzip2), None),
        ("xisf-none", None, Some(XisfCodec::Uncompressed)),
        ("xisf-zstd", None, Some(XisfCodec::Zstd)),
        ("xisf-zlib", None, Some(XisfCodec::Zlib)),
        ("xisf-lz4", None, Some(XisfCodec::Lz4)),
    ] {
        let path = scratch.0.join(format!(
            "{name}.{}",
            if fits.is_some() { "fits" } else { "xisf" }
        ));
        let write_start = Instant::now();
        let fill = |_: usize, start: u64, output: &mut [u16]| -> Result<()> {
            for (i, value) in output.iter_mut().enumerate() {
                *value = pixel(start as usize + i, smooth);
            }
            Ok(())
        };
        if let Some(codec) = fits {
            write_fits_u16(&path, &images, codec, fill)?;
        }
        if let Some(codec) = xisf {
            write_xisf_u16(&path, &images, codec, false, fill)?;
        }
        let write_ms = write_start.elapsed().as_millis();
        let found = inventory(&path, InventoryLimits::default())?;
        let read_start = Instant::now();
        let mut reader = U16ChunkReader::open(&path, &found, 0, PixelReadLimits::default())?;
        let mut buffer = [0u16; 65_536];
        let mut checksum = 0u64;
        for start in (0..WIDTH * HEIGHT).step_by(buffer.len()) {
            let size = (WIDTH * HEIGHT - start).min(buffer.len());
            reader.read_chunk(start as u64, &mut buffer[..size])?;
            for (i, actual) in buffer[..size].iter().enumerate() {
                let expected = pixel(start + i, smooth);
                if *actual != expected {
                    anyhow::bail!("{name}: mismatch at sample {}", start + i);
                }
                checksum = checksum.wrapping_add(u64::from(*actual));
            }
        }
        let read_ms = read_start.elapsed().as_millis();
        let bytes = fs::metadata(&path)
            .with_context(|| format!("Missing {name} output"))?
            .len();
        println!("{name},bytes={bytes},write_ms={write_ms},read_ms={read_ms},checksum={checksum}");
    }
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage writes a complete rusage into the valid out-pointer
        // on success; assume_init is reached only after the zero status.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
            let usage = unsafe { usage.assume_init() };
            #[cfg(target_os = "macos")]
            let bytes = usage.ru_maxrss as u64;
            #[cfg(not(target_os = "macos"))]
            let bytes = (usage.ru_maxrss as u64).saturating_mul(1024);
            println!("process_peak_rss_bytes={bytes}");
        }
    }
    Ok(())
}
