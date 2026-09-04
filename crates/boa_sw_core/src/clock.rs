//! Time source (TS §4.2.1, `AD-9`). The only clock the crate is allowed to read.

#[cfg(feature = "test-util")]
use core::cell::Cell;

/// Time source (TS §4.2.1, `AD-9`). The only clock the crate is allowed to read.
pub trait Clock: 'static {
    /// Wall-clock time, milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
    /// Monotonic milliseconds. MUST never decrease.
    fn monotonic_ms(&self) -> u64;
}

/// Real clock, for hosts (feature `std`).
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct SystemClock {
    /// `Instant` captured at construction for monotonic time.
    start: std::time::Instant,
}

#[cfg(feature = "std")]
impl SystemClock {
    /// Creates a new `SystemClock` capturing the current instant.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

#[cfg(feature = "std")]
impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        // Invariant: a pre-1970 system clock yields 0 (`unwrap_or_default`).
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    fn monotonic_ms(&self) -> u64 {
        self.start
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

/// Deterministic clock for tests (feature `test-util`).
///
/// Interior mutability via `Cell`; the crate is single-threaded by construction (`AD-1`).
#[cfg(feature = "test-util")]
#[derive(Debug)]
pub struct FakeClock {
    wall: Cell<u64>,
    mono: Cell<u64>,
}

#[cfg(feature = "test-util")]
impl FakeClock {
    /// Wall clock and monotonic clock both start at `start_ms`.
    #[must_use]
    pub fn new(start_ms: u64) -> Self {
        Self {
            wall: Cell::new(start_ms),
            mono: Cell::new(start_ms),
        }
    }

    /// Advances both clocks by `delta_ms`.
    pub fn advance_ms(&self, delta_ms: u64) {
        self.wall.set(self.wall.get().wrapping_add(delta_ms));
        self.mono.set(self.mono.get().wrapping_add(delta_ms));
    }

    /// Moves the **wall** clock only; the monotonic clock is untouched, so it never decreases.
    pub fn set_now_ms(&self, value_ms: u64) {
        self.wall.set(value_ms);
    }
}

#[cfg(feature = "test-util")]
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.wall.get()
    }

    fn monotonic_ms(&self) -> u64 {
        self.mono.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "test-util")]
    #[test]
    fn fake_clock_advances_both() {
        let clock = FakeClock::new(1000);
        assert_eq!(clock.now_ms(), 1000);
        assert_eq!(clock.monotonic_ms(), 1000);

        clock.advance_ms(5);
        assert_eq!(clock.now_ms(), 1005);
        assert_eq!(clock.monotonic_ms(), 1005);
    }

    #[cfg(feature = "test-util")]
    #[test]
    fn fake_clock_set_now_does_not_move_monotonic() {
        let clock = FakeClock::new(1000);
        clock.set_now_ms(0);
        assert_eq!(clock.now_ms(), 0);
        assert_eq!(clock.monotonic_ms(), 1000);
    }

    #[cfg(feature = "std")]
    #[test]
    fn system_clock_monotonic_never_decreases() {
        let clock = SystemClock::new();
        let mut prev = clock.monotonic_ms();
        for _ in 0..1000 {
            let current = clock.monotonic_ms();
            assert!(current >= prev);
            prev = current;
        }
    }
}
