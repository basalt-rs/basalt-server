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
            pause_time: Some(Instant::now()),
            total_time_paused: Duration::from_millis(0),
        }
    }
}

pub struct CurrentTime {
    pub paused: bool,
    pub duration: Duration,
}

impl ClockInfo {
    pub fn pause(&mut self) -> bool {
        let affected = self.pause_time.is_some();
        self.pause_time = self.pause_time.or(Some(Instant::now()));
        affected
    }
    pub fn unpause(&mut self) -> bool {
        let affected = self.pause_time.is_none();
        if let Some(pause_time) = self.pause_time {
            self.total_time_paused += pause_time.elapsed();
            self.pause_time = None;
        }
        affected
    }
    pub fn current_time(&self) -> anyhow::Result<CurrentTime> {
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

impl CurrentTime {
    pub fn time_left(self, time_limit: Duration) -> Duration {
        time_limit
            .checked_sub(self.duration)
            .unwrap_or(Duration::from_secs(0))
    }
}
