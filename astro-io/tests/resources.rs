use astro_io::validation::{MemoryBudget, ValidationErrorKind};

#[test]
fn reservations_distinguish_busy_from_impossible_and_release_on_unwind() {
    assert!(MemoryBudget::new(0).is_err());
    let budget = MemoryBudget::new(100).unwrap();
    let first = budget.try_reserve(60).unwrap();
    assert_eq!(budget.used_bytes(), 60);
    assert_eq!(
        budget.try_reserve(41).unwrap_err().kind(),
        ValidationErrorKind::ResourceBusy
    );
    assert_eq!(
        budget.try_reserve(101).unwrap_err().kind(),
        ValidationErrorKind::ResourceLimit
    );
    drop(first);
    let _ = std::panic::catch_unwind(|| {
        let _p = budget.try_reserve(80).unwrap();
        panic!("test unwind");
    });
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(budget.peak_bytes(), 80);
}

#[test]
fn simultaneous_callers_cannot_overbook_and_clones_share_capacity() {
    let budget = MemoryBudget::new(64).unwrap();
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let budget = budget.clone();
            let barrier = &barrier;
            scope.spawn(move || {
                let reservation = budget.try_reserve(16);
                barrier.wait();
                assert_eq!(budget.used_bytes(), 64);
                barrier.wait();
                drop(reservation);
            });
        }
    });
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(budget.peak_bytes(), 64);
}
