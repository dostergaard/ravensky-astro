use astro_bench::{FixtureSet, Recipe};
use std::{fs, process::Command, sync::atomic::AtomicBool};

#[test]
fn cli_runs_isolated_samples_and_preserves_existing_outputs() {
    let owner = FixtureSet::generate(
        &std::env::temp_dir(),
        Recipe {
            width: 2,
            height: 2,
            frames: 1,
            ..Recipe::default()
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let output = owner.directory().join("report.json");
    let arguments = [
        "run",
        "--width",
        "32",
        "--height",
        "32",
        "--frames",
        "3",
        "--workers",
        "1,2",
        "--repeats",
        "2",
        "--encoding",
        "zstd",
        "--workload",
        "full",
        "--scratch",
        owner.directory().to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ];
    let result = Command::new(env!("CARGO_BIN_EXE_astro-bench"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["samples"].as_array().unwrap().len(), 4);
    assert_eq!(report["summaries"].as_array().unwrap().len(), 2);
    assert_eq!(report["samples"][0]["completed_files"], 3);
    assert_eq!(
        report["provenance"]["source_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let result = Command::new(env!("CARGO_BIN_EXE_astro-bench"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    assert!(!fs::read_dir(owner.directory()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("astro-bench-")));
    assert!(owner.directory().join("manifest.json").exists());
}

#[test]
fn cli_rejects_excessive_work_before_generating() {
    let result = Command::new(env!("CARGO_BIN_EXE_astro-bench"))
        .args(["run", "--workers", "999", "--output", "unused.json"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("workers"));
}
