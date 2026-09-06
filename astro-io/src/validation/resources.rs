use super::*;
use std::sync::{atomic::AtomicU64, Arc};

/// Caller-owned nonblocking allowance shared by concurrent validation calls.
///
/// Clones refer to the same counters. Reservations cover managed buffers and
/// documented parser/codec allowances, not OS memory, allocator metadata, stacks
/// or the entire process. The caller handles queuing, fairness and pressure policy.
#[derive(Debug, Clone)]
pub struct MemoryBudget(Arc<State>);
#[derive(Debug)]
struct State {
    capacity: u64,
    used: AtomicU64,
    peak: AtomicU64,
}
impl MemoryBudget {
    /// Create a finite, nonzero byte allowance. Does not allocate the allowance.
    pub fn new(capacity: u64) -> Result<Self> {
        nonzero(capacity)?;
        Ok(Self(Arc::new(State {
            capacity,
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
        })))
    }
    /// Fixed total allowance in bytes.
    pub fn capacity_bytes(&self) -> u64 {
        self.0.capacity
    }
    /// Currently reserved bytes, observed atomically.
    pub fn used_bytes(&self) -> u64 {
        self.0.used.load(Ordering::Relaxed)
    }
    /// Lifetime high-water reservation count (not RSS).
    pub fn peak_bytes(&self) -> u64 {
        self.0.peak.load(Ordering::Relaxed)
    }
    /// Reserve before allocating caller-owned working data. Never waits.
    /// Returns `ResourceLimit` if the request cannot fit an empty allowance,
    /// or `ResourceBusy` if existing reservations temporarily prevent admission.
    pub fn try_reserve(&self, bytes: u64) -> Result<MemoryReservation> {
        if bytes > self.0.capacity {
            return Err(limit("request exceeds total shared memory capacity"));
        }
        let mut used = self.used_bytes();
        loop {
            if bytes > self.0.capacity - used {
                return Err(ValidationError::new(
                    ValidationErrorKind::ResourceBusy,
                    "shared memory busy; retry after active reservations are released",
                ));
            }
            match self.0.used.compare_exchange_weak(
                used,
                used + bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.0.peak.fetch_max(used + bytes, Ordering::Relaxed);
                    break;
                }
                Err(actual) => used = actual,
            }
        }
        Ok(MemoryReservation {
            budget: self.clone(),
            bytes,
        })
    }
}
/// An owned reservation; dropping it releases capacity on every exit path.
/// Keep it alive until the memory it represents is dropped. It is not cloneable.
#[derive(Debug)]
pub struct MemoryReservation {
    budget: MemoryBudget,
    bytes: u64,
}
impl MemoryReservation {
    /// Number of bytes owned by this reservation.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    fn absorb(&mut self, mut other: Self) {
        self.bytes += other.bytes;
        other.bytes = 0;
    }
}
impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.budget.0.used.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(super) struct Account {
    local: MemoryBudget,
    shared: MemoryBudget,
}
impl Account {
    pub fn new(limit: u64, shared: &MemoryBudget) -> Result<Self> {
        Ok(Self {
            local: MemoryBudget::new(limit)?,
            shared: shared.clone(),
        })
    }
    pub fn remaining(&self) -> u64 {
        self.local
            .capacity_bytes()
            .min(self.shared.capacity_bytes())
            .saturating_sub(self.local.used_bytes())
    }
    pub fn peak(&self) -> u64 {
        self.local.peak_bytes()
    }
    pub fn reserve(&self, bytes: u64) -> Result<Reservation> {
        if bytes > self.remaining() {
            return Err(limit(
                "stage plus retained memory exceeds per-call or total shared capacity",
            ));
        }
        let local = self
            .local
            .try_reserve(bytes)
            .map_err(|_| limit("per-call working memory exceeded"))?;
        let shared = self.shared.try_reserve(bytes)?;
        Ok(Reservation { local, shared })
    }
    pub fn buffer(&self, size: usize) -> Result<Buffer> {
        let reservation = self.reserve(size as u64)?;
        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|_| limit("buffer allocation failed"))?;
        data.resize(size, 0);
        Ok(Buffer {
            data,
            _reservation: reservation,
        })
    }
}
pub(super) struct Reservation {
    local: MemoryReservation,
    shared: MemoryReservation,
}
impl Reservation {
    /// Grow before metadata allocation; contention unwinds the enclosing call.
    pub fn grow(&mut self, bytes: u64) -> Result<()> {
        let account = Account {
            local: self.local.budget.clone(),
            shared: self.shared.budget.clone(),
        };
        let extra = account.reserve(bytes)?;
        self.local.absorb(extra.local);
        self.shared.absorb(extra.shared);
        Ok(())
    }
}
pub(super) struct Buffer {
    data: Vec<u8>,
    _reservation: Reservation,
}
impl Buffer {
    pub fn truncate(&mut self, length: usize) {
        self.data.truncate(length);
    }
}
impl std::ops::Deref for Buffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.data
    }
}
impl std::ops::DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}
