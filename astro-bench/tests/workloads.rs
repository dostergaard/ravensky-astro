use astro_bench::{run_sample, Encoding, FixtureSet, Pattern, Recipe, Workload};
use std::sync::atomic::AtomicBool;

#[test]
fn compressed_fits_fixtures_match_native_pixels_and_are_reproducible() {
    let cancel = AtomicBool::new(false);
    for pattern in [Pattern::Noise, Pattern::Gradient] {
        let raw = FixtureSet::generate(
            &std::env::temp_dir(),
            Recipe {
                width: 7,
                height: 5,
                frames: 2,
                pattern,
                ..Recipe::default()
            },
            &cancel,
        )
        .unwrap();
        for encoding in ["fits_gzip", "fits_gzip2"] {
            for rows in [None, Some(1), Some(3)] {
                let mut json = serde_json::to_value(&raw.manifest().recipe).unwrap();
                json["encoding"] = encoding.into();
                if let Some(rows) = rows {
                    json["tile_rows"] = rows.into();
                }
                let recipe: Recipe = serde_json::from_value(json).unwrap();
                let set =
                    FixtureSet::generate(&std::env::temp_dir(), recipe.clone(), &cancel).unwrap();
                let again = FixtureSet::generate(&std::env::temp_dir(), recipe, &cancel).unwrap();
                assert_eq!(set.manifest().generator_version, 2);
                assert_eq!(set.manifest().files, again.manifest().files);
                FixtureSet::open(set.directory(), &cancel).unwrap();
                for (original, compressed) in raw.manifest().files.iter().zip(&set.manifest().files)
                {
                    let bytes = std::fs::read(raw.directory().join(&original.name)).unwrap();
                    let expected: Vec<_> = bytes[2880..2880 + 70]
                        .chunks_exact(2)
                        .map(|b| u16::from_be_bytes([b[0], b[1]]) ^ 0x8000)
                        .collect();
                    astro_io::fits::backend::with_cfitsio(|| {
                        let mut file =
                            fitsio::FitsFile::open(set.directory().join(&compressed.name)).unwrap();
                        let hdu = file.hdu(1).unwrap();
                        let actual: Vec<u16> = hdu.read_image(&mut file).unwrap();
                        assert_eq!(actual, expected, "{encoding}, tile rows {rows:?}");
                    });
                }
                for workload in [Workload::Read, Workload::Structural, Workload::Full] {
                    let result = run_sample(&set, workload, 2, &cancel, |_| {}).unwrap();
                    assert_eq!(result.completed_files, 2);
                    assert_eq!(result.decoded_bytes, 140);
                }
            }
        }
    }
}

#[test]
fn tile_options_and_expansion_overhead_are_checked_before_generation() {
    for recipe in [
        Recipe {
            tile_rows: Some(1),
            ..Recipe::default()
        },
        Recipe {
            encoding: Encoding::FitsGzip,
            tile_rows: Some(0),
            ..Recipe::default()
        },
        Recipe {
            encoding: Encoding::FitsGzip2,
            tile_rows: Some(2049),
            ..Recipe::default()
        },
        Recipe {
            width: 1,
            height: 16385,
            encoding: Encoding::FitsGzip,
            tile_rows: Some(1),
            ..Recipe::default()
        },
        // Tiny tiles require headers/trailers/descriptors beyond the original
        // whole-image expansion allowance, even with only 32 KiB of pixels.
        Recipe {
            width: 1,
            height: 16384,
            frames: 1,
            encoding: Encoding::FitsGzip,
            tile_rows: Some(1),
            max_disk_bytes: 2 * 1024 * 1024,
            ..Recipe::default()
        },
    ] {
        assert!(
            FixtureSet::generate(&std::env::temp_dir(), recipe, &AtomicBool::new(false)).is_err()
        );
    }
}

#[test]
fn original_generator_bytes_and_manifest_version_are_preserved() {
    let cancel = AtomicBool::new(false);
    let set = FixtureSet::generate(
        &std::env::temp_dir(),
        Recipe {
            frames: 1,
            ..Recipe::default()
        },
        &cancel,
    )
    .unwrap();
    // Recorded by the pre-streaming benchmark, before this pixel-generator refactor.
    assert_eq!(
        set.manifest().files[0].sha256,
        "859f4ef6fed5ab66521611ebda51ae8e327a5c0fa3d75aec605dc068192a72d9"
    );
    assert_eq!(set.manifest().generator_version, 1);
    let json = serde_json::to_value(set.manifest()).unwrap();
    assert!(json["recipe"].get("tile_rows").is_none());
    FixtureSet::open(set.directory(), &cancel).unwrap();
}
#[test]
fn fixtures_are_reproducible_valid_and_removed_on_drop() {
    let parent = std::env::temp_dir();
    let cancel = AtomicBool::new(false);
    let recipe = Recipe {
        width: 32,
        height: 32,
        frames: 3,
        ..Recipe::default()
    };
    for encoding in [
        Encoding::Fits,
        Encoding::Xisf,
        Encoding::Zlib,
        Encoding::Zstd,
    ] {
        let r = Recipe {
            encoding,
            ..recipe.clone()
        };
        let a = FixtureSet::generate(&parent, r.clone(), &cancel).unwrap();
        let b = FixtureSet::generate(&parent, r, &cancel).unwrap();
        assert_eq!(a.manifest().files, b.manifest().files);
        let result = run_sample(&a, Workload::Full, 2, &cancel, |_| {}).unwrap();
        assert_eq!(result.completed_files, 3);
        assert_eq!(result.decoded_bytes, 3 * 32 * 32 * 2);
        let path = a.directory().to_owned();
        drop(a);
        assert!(!path.exists());
    }
}
#[test]
fn invalid_budgets_and_cancellation_do_not_create_fixtures() {
    let parent = std::env::temp_dir();
    let cancel = AtomicBool::new(false);
    for r in [
        Recipe {
            width: 0,
            ..Recipe::default()
        },
        Recipe {
            max_disk_bytes: 1,
            ..Recipe::default()
        },
    ] {
        assert!(FixtureSet::generate(&parent, r, &cancel).is_err());
    }
    assert!(FixtureSet::generate(&parent, Recipe::default(), &AtomicBool::new(true)).is_err());
}
#[test]
fn noise_changes_with_seed_and_raw_io_counts_bytes() {
    let parent = std::env::temp_dir();
    let c = AtomicBool::new(false);
    let r = Recipe {
        width: 64,
        height: 64,
        frames: 2,
        pattern: Pattern::Noise,
        ..Recipe::default()
    };
    let a = FixtureSet::generate(&parent, r.clone(), &c).unwrap();
    let b = FixtureSet::generate(
        &parent,
        Recipe {
            seed: r.seed + 1,
            ..r
        },
        &c,
    )
    .unwrap();
    assert_ne!(a.manifest().files[0].sha256, b.manifest().files[0].sha256);
    let result = run_sample(&a, Workload::Read, 1, &c, |_| {}).unwrap();
    assert_eq!(result.read_bytes, result.stored_bytes);
    assert!(run_sample(&a, Workload::Full, 0, &c, |_| {}).is_err());
    assert!(run_sample(&a, Workload::Full, 1, &AtomicBool::new(true), |_| {}).is_err());
}

#[test]
fn manifest_tampering_and_changed_content_are_rejected_without_deleting_owned_files() {
    let c = AtomicBool::new(false);
    let set = FixtureSet::generate(
        &std::env::temp_dir(),
        Recipe {
            width: 8,
            height: 8,
            frames: 1,
            ..Recipe::default()
        },
        &c,
    )
    .unwrap();
    let borrowed = FixtureSet::open(set.directory(), &c).unwrap();
    drop(borrowed);
    assert!(set.directory().exists());
    let path = set.directory().join("manifest.json");
    let original = std::fs::read(&path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    manifest["files"][0]["name"] = "../outside.fits".into();
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(FixtureSet::open(set.directory(), &c).is_err());
    std::fs::write(&path, original).unwrap();
    let image = set.directory().join(&set.manifest().files[0].name);
    let mut bytes = std::fs::read(&image).unwrap();
    bytes[2880] ^= 1;
    std::fs::write(&image, bytes).unwrap();
    assert!(FixtureSet::open(set.directory(), &c).is_err());
    assert!(image.exists());
}

#[test]
fn cancellation_and_file_failures_are_not_successful_samples() {
    use std::sync::atomic::Ordering;
    let c = AtomicBool::new(false);
    let set = FixtureSet::generate(
        &std::env::temp_dir(),
        Recipe {
            width: 16,
            height: 16,
            frames: 32,
            ..Recipe::default()
        },
        &c,
    )
    .unwrap();
    let caller = std::thread::current().id();
    let result = run_sample(&set, Workload::Full, 2, &c, |_| {
        assert_eq!(caller, std::thread::current().id());
        c.store(true, Ordering::Relaxed);
    });
    assert!(result.is_err());
    c.store(false, Ordering::Relaxed);
    std::fs::remove_file(set.directory().join(&set.manifest().files[0].name)).unwrap();
    assert!(run_sample(&set, Workload::Read, 4, &c, |_| {}).is_err());
    assert!(Workload::Full
        .check_budget(
            &Recipe {
                width: 8192,
                height: 4096,
                encoding: Encoding::Zstd,
                ..Recipe::default()
            },
            4
        )
        .is_err());
    assert!(Workload::Read.check_budget(&Recipe::default(), 17).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_fixtures_are_rejected() {
    let c = AtomicBool::new(false);
    let set = FixtureSet::generate(
        &std::env::temp_dir(),
        Recipe {
            width: 8,
            height: 8,
            frames: 1,
            ..Recipe::default()
        },
        &c,
    )
    .unwrap();
    let path = set.directory().join(&set.manifest().files[0].name);
    let renamed = set.directory().join("original");
    std::fs::rename(&path, &renamed).unwrap();
    std::os::unix::fs::symlink(&renamed, &path).unwrap();
    assert!(FixtureSet::open(set.directory(), &c).is_err());
}

#[test]
fn fits_and_xisf_store_the_same_logical_pixels_and_workers_visit_each_file_once() {
    let c = AtomicBool::new(false);
    for pattern in [Pattern::Noise, Pattern::Gradient] {
        let recipe = Recipe {
            width: 16,
            height: 16,
            frames: 5,
            pattern,
            ..Recipe::default()
        };
        let fits = FixtureSet::generate(&std::env::temp_dir(), recipe.clone(), &c).unwrap();
        let xisf = FixtureSet::generate(
            &std::env::temp_dir(),
            Recipe {
                encoding: Encoding::Xisf,
                ..recipe
            },
            &c,
        )
        .unwrap();
        let a = std::fs::read(fits.directory().join(&fits.manifest().files[0].name)).unwrap();
        let b = std::fs::read(xisf.directory().join(&xisf.manifest().files[0].name)).unwrap();
        for (a, b) in a[2880..2880 + 512]
            .chunks_exact(2)
            .zip(b[4096..].chunks_exact(2))
        {
            assert_eq!(
                u16::from_be_bytes([a[0], a[1]]) ^ 0x8000,
                u16::from_le_bytes([b[0], b[1]])
            );
        }
        for workload in [Workload::Read, Workload::Structural, Workload::Full] {
            let mut seen = Vec::new();
            let result = run_sample(&xisf, workload, 4, &c, |f| seen.push(f.index)).unwrap();
            seen.sort_unstable();
            assert_eq!(seen, vec![0, 1, 2, 3, 4]);
            assert_eq!(
                result.files.iter().map(|f| f.index).collect::<Vec<_>>(),
                seen
            );
            assert_eq!(result.completed_files, 5);
        }
    }
}
