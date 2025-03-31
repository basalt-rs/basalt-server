use std::time::{Duration, Instant};

use anyhow::Context;

/// Tracks all information related to the in-game clock
pub struct ClockInfo {
    /// Time when the competition began
    pub start_time: Instant,
    /// Time that the competition has been paused.
    /// One can infer that the competition is paused if this value is `Some`
    pub pause_time: Option<Instant>,
    /// Total duration the competition has been paused.
    pub total_time_paused: Duration,
}

impl Default for ClockInfo {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            pause_time: None,
            total_time_paused: Duration::from_millis(0),
        }
    }
}

pub struct CurrentTime {
    pub paused: bool,
    pub duration: Duration,
}

impl ClockInfo {
    fn pause(&mut self) {
        self.pause_time = self.pause_time.or(Some(Instant::now()));
    }
    fn unpause(&mut self) {
        if let Some(pause_time) = self.pause_time {
            self.total_time_paused += pause_time.elapsed();
        }
    }
    fn current_time(self) -> anyhow::Result<CurrentTime> {
        match self.pause_time {
            Some(pause_time) => Ok(CurrentTime {
                paused: true,
                duration: pause_time.elapsed(),
            }),
            None => {
                let duration = self
                    .start_time
                    .checked_add(self.total_time_paused)
                    .context("Failed to add pause duration to start time")?
                    .elapsed();
                Ok(CurrentTime {
                    paused: false,
                    duration,
                })
            }
        }
    }
}
