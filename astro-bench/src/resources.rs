// Keep platform FFI isolated. No allocation or process-global signal behavior.
#[derive(Default)]
pub(crate) struct Usage {
    pub cpu_seconds: Option<f64>,
    pub peak_rss_bytes: Option<u64>,
}
pub(crate) fn usage() -> Usage {
    #[cfg(unix)]
    {
        let mut value = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage writes a complete rusage to a valid aligned pointer;
        // assume_init is reached only after the OS reports successful initialization.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, value.as_mut_ptr()) } != 0 {
            return Usage::default();
        }
        // SAFETY: the successful call above initialized value.
        let value = unsafe { value.assume_init() };
        let cpu_seconds = Some(
            value.ru_utime.tv_sec as f64
                + value.ru_stime.tv_sec as f64
                + (value.ru_utime.tv_usec as f64 + value.ru_stime.tv_usec as f64) / 1_000_000.0,
        );
        #[cfg(target_os = "macos")]
        let peak_rss_bytes = u64::try_from(value.ru_maxrss).ok();
        #[cfg(target_os = "linux")]
        let peak_rss_bytes = u64::try_from(value.ru_maxrss)
            .ok()
            .and_then(|n| n.checked_mul(1024));
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let peak_rss_bytes = None;
        Usage {
            cpu_seconds,
            peak_rss_bytes,
        }
    }
    #[cfg(not(unix))]
    Usage::default()
}
