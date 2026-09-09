//! Coordinated access to the linked CFITSIO backend.
//!
//! Path-based RavenSky helpers participate automatically. When using `fitsio`
//! directly, enclose **open, all operations, error handling and close/drop** in
//! [`with_cfitsio`] (or [`try_with_cfitsio`]). Do not return a live handle from the
//! closure. Borrowed-handle helpers cannot protect opens/closes performed outside
//! this protocol. Separate linked copies and unrelated native callers must also
//! be coordinated by the application. Concurrent readers need independent handles.

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, TryLockError};
use std::thread::{self, ThreadId};

/// Whether the linked CFITSIO reports support for concurrent independent handles.
///
/// This is a native build capability, not a memory limit or a guarantee about
/// callers sharing handles or concurrently writing the same file.
pub fn is_reentrant() -> bool {
    gate().reentrant
}

/// Run native FITS work, waiting for admission on non-reentrant builds.
///
/// Nested calls on the same thread are supported. Keep the complete handle
/// lifetime inside the closure. Blocking admission and native I/O cannot be
/// cancelled; background schedulers should prefer [`try_with_cfitsio`].
/// Do not wait inside the closure for another thread that needs native admission.
pub fn with_cfitsio<T>(operation: impl FnOnce() -> T) -> T {
    let _permit = gate().acquire();
    operation()
}

/// Run native FITS work if admission is immediately available.
///
/// Returns [`CfitsioBusy`] without invoking the closure when another thread owns
/// a non-reentrant backend (or its admission state is momentarily locked).
/// Retrying/fairness belong to the caller. Reentrant builds admit all calls.
/// The complete native handle lifetime must remain inside the closure.
pub fn try_with_cfitsio<T>(operation: impl FnOnce() -> T) -> Result<T, CfitsioBusy> {
    let _permit = gate().try_acquire()?;
    Ok(operation())
}

fn gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| {
        // SAFETY: this no-argument function only returns the compiled _REENTRANT
        // constant. It accesses no file handle or mutable native state.
        Gate::new(unsafe { fitsio::sys::fits_is_reentrant() } != 0)
    })
}

/// Another thread is using a non-reentrant CFITSIO backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfitsioBusy;

impl fmt::Display for CfitsioBusy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CFITSIO is busy on another thread; retry after active native work completes")
    }
}

impl std::error::Error for CfitsioBusy {}

struct Gate {
    reentrant: bool,
    state: Mutex<State>,
    available: Condvar,
}

#[derive(Default)]
struct State {
    owner: Option<ThreadId>,
    depth: usize,
}

// A permit must be released by its owning thread, including during unwinding.
struct Permit<'a> {
    gate: &'a Gate,
    _same_thread: PhantomData<Rc<()>>,
}

impl Gate {
    fn new(reentrant: bool) -> Self {
        Self {
            reentrant,
            state: Mutex::new(State::default()),
            available: Condvar::new(),
        }
    }

    fn permit(&self) -> Permit<'_> {
        Permit {
            gate: self,
            _same_thread: PhantomData,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // User/native code never runs with this mutex held; ownership bookkeeping
        // remains usable after unwinding. No protected native data is recovered.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn acquire(&self) -> Permit<'_> {
        if !self.reentrant {
            let owner = thread::current().id();
            let mut state = self.state();
            while state.owner.is_some_and(|id| id != owner) {
                state = self
                    .available
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
            state.owner = Some(owner);
            state.depth += 1;
        }
        self.permit()
    }

    fn try_acquire(&self) -> Result<Permit<'_>, CfitsioBusy> {
        if !self.reentrant {
            let owner = thread::current().id();
            let mut state = match self.state.try_lock() {
                Ok(state) => state,
                Err(TryLockError::WouldBlock) => return Err(CfitsioBusy),
                Err(TryLockError::Poisoned(e)) => e.into_inner(),
            };
            if state.owner.is_some_and(|id| id != owner) {
                return Err(CfitsioBusy);
            }
            state.owner = Some(owner);
            state.depth += 1;
        }
        Ok(self.permit())
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if !self.gate.reentrant {
            let mut state = self.gate.state();
            state.depth -= 1;
            if state.depth == 0 {
                state.owner = None;
                self.gate.available.notify_one();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Barrier};
    use std::thread;

    #[test]
    fn serial_gate_rejects_other_threads_but_allows_nested_calls() {
        let gate = Gate::new(false);
        let outer = gate.acquire();
        let inner = gate.try_acquire().unwrap();
        thread::scope(|s| {
            assert!(s.spawn(|| gate.try_acquire().is_err()).join().unwrap());
        });
        drop(inner);
        thread::scope(|s| {
            assert!(s.spawn(|| gate.try_acquire().is_err()).join().unwrap());
        });
        drop(outer);
        thread::scope(|s| {
            assert!(s.spawn(|| gate.try_acquire().is_ok()).join().unwrap());
        });
    }

    #[test]
    fn serial_gate_releases_after_unwinding() {
        let gate = Gate::new(false);
        let result = std::panic::catch_unwind(|| {
            let _permit = gate.acquire();
            panic!("exercise release");
        });
        assert!(result.is_err());
        thread::scope(|s| {
            assert!(s.spawn(|| gate.try_acquire().is_ok()).join().unwrap());
        });
    }

    #[test]
    fn waiting_caller_enters_after_owner_releases() {
        let gate = Gate::new(false);
        let permit = gate.acquire();
        let (tx, rx) = mpsc::channel();
        thread::scope(|s| {
            let worker = s.spawn(|| {
                assert!(gate.try_acquire().is_err());
                tx.send(()).unwrap();
                let _permit = gate.acquire();
                tx.send(()).unwrap();
            });
            rx.recv().unwrap();
            assert!(rx.try_recv().is_err());
            drop(permit);
            rx.recv().unwrap();
            worker.join().unwrap();
        });
    }

    #[test]
    fn reentrant_gate_allows_simultaneous_callers() {
        let gate = Gate::new(true);
        let barrier = Barrier::new(4);
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let _permit = gate.try_acquire().unwrap();
                    barrier.wait();
                });
            }
        });
    }
}
