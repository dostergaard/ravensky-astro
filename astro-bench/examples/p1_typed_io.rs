//! Reproducible P1 UInt16 writer/reader probe: 1024x1024 monochrome patterns.
//! Also used for bounded P4 codec characterization. `--output REPORT.json`
//! records direct encode/decode timing and conservative FITS bounds.

use anyhow::{ensure, Context, Result};
use astro_io::typed::write::{
    fits_output_bound_u16, write_fits_u16, write_xisf_u16, EncodedSizeBound, FitsCodec,
    U16ImageSpec, XisfCodec,
};
use astro_io::typed::{inventory, InventoryLimits, PixelReadLimits, U16ChunkReader};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
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
        let available = fs4::available_space(std::env::temp_dir())?;
        ensure!(
            available >= (128 << 20) + (1 << 30),
            "typed-I/O probe refused: 128 MiB scratch budget plus 1 GiB host headroom unavailable"
        );
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
    let mut smooth = false;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smooth" => smooth = true,
            "--output" => {
                output = Some(PathBuf::from(args.next().context("--output needs a path")?))
            }
            _ => anyhow::bail!("unknown argument {arg}"),
        }
    }
    if let Some(path) = &output {
        ensure!(!path.exists(), "report already exists");
    }
    println!("pattern={}", if smooth { "smooth" } else { "gradient" });
    let scratch = Scratch::new()?;
    let images = [U16ImageSpec {
        id: Some("LIGHT"),
        width: WIDTH,
        height: HEIGHT,
    }];
    let mut cases = Vec::new();
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
        let bound = if let Some(codec) = fits {
            match fits_output_bound_u16(&images, codec)? {
                EncodedSizeBound::Finite(value) => {
                    ensure!(bytes <= value, "{name} exceeds FITS bound");
                    Some(value)
                }
                EncodedSizeBound::Unknown(reason) => {
                    anyhow::bail!("{name} bound unknown: {reason}")
                }
            }
        } else {
            None
        };
        cases.push(
            json!({"name":name,"stored_bytes":bytes,"decoded_bytes":WIDTH*HEIGHT*2,
            "write_milliseconds":write_ms,"readback_milliseconds":read_ms,
            "output_bound_bytes":bound,"checksum":checksum,"exact_readback":true}),
        );
        println!("{name},bytes={bytes},write_ms={write_ms},read_ms={read_ms},checksum={checksum}");
    }
    let mut peak_rss = None;
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
            peak_rss = Some(bytes);
        }
    }
    if let Some(path) = output {
        let report = json!({"schema_version":1,"workload":"direct-typed-u16-codecs",
            "pattern":if smooth {"smooth"} else {"gradient"},"width":WIDTH,"height":HEIGHT,
            "cases":cases,"process_lifetime_peak_rss_bytes":peak_rss,
            "scratch_budget_bytes":128u64<<20,"host_headroom_bytes":1u64<<30,
            "cache_state":"uncontrolled, sequential cases in one process",
            "os":std::env::consts::OS,"architecture":std::env::consts::ARCH});
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        file.sync_all()?;
    }
    Ok(())
}
