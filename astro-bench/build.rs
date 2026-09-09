use sha2::{Digest, Sha256};
use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect(&entry?.path(), files)?;
        }
    } else {
        files.push(path.to_owned());
    }
    Ok(())
}
fn command(root: &Path, name: &str, args: &[&str]) -> String {
    Command::new(name)
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unavailable".into())
}
fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest.parent().ok_or("missing workspace parent")?;
    let mut files = Vec::new();
    // Explicit scope includes the measured validator, runner/generator and lockfile.
    // Docs and baseline artifacts do not change the measured-source fingerprint.
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "astro-io/Cargo.toml",
        "astro-io/src",
        "astro-bench/Cargo.toml",
        "astro-bench/build.rs",
        "astro-bench/src",
    ] {
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        collect(&path, &mut files)?;
    }
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        let name = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(&path)?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    println!("cargo:rustc-env=BENCH_SOURCE_SHA256={:x}", hash.finalize());
    println!(
        "cargo:rustc-env=BENCH_REVISION={}",
        command(root, "git", &["rev-parse", "HEAD"])
    );
    println!("cargo:rustc-env=BENCH_PROFILE={}", env::var("PROFILE")?);
    let rustc = env::var("RUSTC")?;
    println!(
        "cargo:rustc-env=BENCH_RUSTC={}",
        command(root, &rustc, &["--version"])
    );
    // Git metadata is supplementary; the content hash also covers uncommitted sources.
    let git_head = command(root, "git", &["rev-parse", "--git-path", "HEAD"]);
    if git_head != "unavailable" {
        println!("cargo:rerun-if-changed={}", root.join(git_head).display());
    }
    let git_ref = command(root, "git", &["symbolic-ref", "-q", "HEAD"]);
    if git_ref != "unavailable" {
        let path = command(root, "git", &["rev-parse", "--git-path", &git_ref]);
        if path != "unavailable" {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    Ok(())
}
