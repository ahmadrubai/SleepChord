use std::time::{Duration, Instant};

/// A monotonic deadline keeps the countdown independent of UI frame timing.
#[derive(Default)]
pub(super) struct SleepTimer {
    deadline: Option<Instant>,
}

impl SleepTimer {
    pub(super) fn start(&mut self, duration: Duration) {
        self.deadline = Some(Instant::now() + duration);
    }

    pub(super) fn cancel(&mut self) {
        self.deadline = None;
    }

    pub(super) fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    /// Returns true once, clearing the deadline before the caller stops playback.
    pub(super) fn take_expired(&mut self) -> bool {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_and_cancelling_timer_discards_old_deadline() {
        let mut timer = SleepTimer::default();
        timer.start(Duration::from_secs(60));
        timer.start(Duration::from_secs(600));
        assert!(
            timer
                .remaining()
                .is_some_and(|d| d > Duration::from_secs(590))
        );
        timer.cancel();
        assert_eq!(timer.remaining(), None);
        assert!(!timer.take_expired());
    }

    #[test]
    fn expired_timer_is_consumed_once() {
        let mut timer = SleepTimer {
            deadline: Some(Instant::now() - Duration::from_secs(1)),
        };
        assert!(timer.take_expired());
        assert!(!timer.take_expired());
        assert_eq!(timer.remaining(), None);
    }
}
