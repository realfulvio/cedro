//! One heavy job per process, shared by CLI, wheels and every panel.
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

static BUSY: Mutex<bool> = Mutex::new(false);
static READY: Condvar = Condvar::new();
thread_local! { static DEPTH: Cell<usize> = const { Cell::new(0) }; }

// A guard must be released on the acquiring thread (recursive archive conversions).
pub struct JobGuard(std::marker::PhantomData<std::rc::Rc<()>>);

/// Non-blocking admission for an automatic updater: never interrupt active work.
pub fn try_acquire() -> Option<JobGuard> {
    if DEPTH.with(|d| d.get() > 0) {
        return acquire(None);
    }
    let mut busy = match BUSY.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    if *busy {
        return None;
    }
    *busy = true;
    DEPTH.with(|d| d.set(1));
    Some(JobGuard(std::marker::PhantomData))
}

pub fn acquire(cancel: Option<&AtomicBool>) -> Option<JobGuard> {
    if DEPTH.with(|d| d.get() > 0) {
        DEPTH.with(|d| d.set(d.get() + 1));
        return Some(JobGuard(std::marker::PhantomData));
    }
    let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return None;
        }
        if !*busy {
            break;
        }
        busy = READY
            .wait_timeout(busy, Duration::from_millis(100))
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    *busy = true;
    DEPTH.with(|d| d.set(1));
    Some(JobGuard(std::marker::PhantomData))
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        let depth = DEPTH.with(|d| {
            let n = d.get() - 1;
            d.set(n);
            n
        });
        if depth == 0 {
            *BUSY.lock().unwrap_or_else(|e| e.into_inner()) = false;
            READY.notify_all();
        }
    }
}

/// Live free physical RAM, never the model's download size or installed RAM.
pub fn available_memory() -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        return GlobalMemoryStatusEx(&mut m).ok().map(|_| m.ullAvailPhys);
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        return text
            .lines()
            .find_map(|l| {
                l.strip_prefix("MemAvailable:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .map(|kb| kb * 1024);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    None
}

/// Conservative extra headroom for GPU buffers, including shared-memory iGPUs.
/// This changes the backend only: model and decoding parameters remain identical.
pub fn gpu_headroom(free: Option<u64>, model_bytes: u64) -> bool {
    free.is_none_or(|n| {
        n >= model_bytes
            .saturating_mul(3)
            .saturating_add(512 * 1024 * 1024)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_jobs_and_cancellation() {
        let first = acquire(None).unwrap();
        let nested = acquire(None).unwrap();
        drop(nested);
        let cancelled = AtomicBool::new(true);
        std::thread::scope(|s| {
            s.spawn(|| assert!(acquire(Some(&cancelled)).is_none()))
                .join()
                .unwrap();
        });
        drop(first);
        assert!(acquire(None).is_some());
    }
    #[test]
    fn updater_cannot_enter_during_another_job() {
        let held = acquire(None).unwrap();
        std::thread::spawn(|| assert!(try_acquire().is_none()))
            .join()
            .unwrap();
        assert!(try_acquire().is_some()); // Recursive admission on the owner thread.
        drop(held);
        let cancelled = AtomicBool::new(false);
        assert!(acquire(Some(&cancelled)).is_some());
    }
    #[test]
    fn memory_policy_keeps_gpu_when_budget_allows() {
        assert!(gpu_headroom(Some(4 << 30), 574_000_000));
        assert!(!gpu_headroom(Some(1 << 30), 574_000_000));
        assert!(gpu_headroom(None, 574_000_000));
    }
    #[test]
    fn concurrent_surfaces_never_overlap() {
        let active = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..4 {
                let active = &active;
                s.spawn(move || {
                    let _guard = acquire(None).unwrap();
                    assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                    std::thread::sleep(Duration::from_millis(5));
                    assert_eq!(active.fetch_sub(1, Ordering::SeqCst), 1);
                });
            }
        });
    }
}
