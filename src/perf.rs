use std::collections::VecDeque;
use std::time::Duration;

pub const BUDGET_120FPS: Duration = Duration::from_micros(8333);

pub struct FrameTimer {
    samples: VecDeque<Duration>,
    max_samples: usize,
}

impl FrameTimer {
    pub fn new(max_samples: usize) -> Self {
        Self { samples: VecDeque::with_capacity(max_samples), max_samples }
    }

    pub fn record(&mut self, frame_time: Duration) {
        if self.samples.len() == self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(frame_time);
    }

    pub fn average_frame_time(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.samples.iter().sum();
        total / self.samples.len() as u32
    }

    pub fn fps(&self) -> f32 {
        let avg = self.average_frame_time();
        if avg.as_secs_f32() <= 0.0 {
            0.0
        } else {
            1.0 / avg.as_secs_f32()
        }
    }

    pub fn meets_budget(&self, budget: Duration) -> bool {
        self.average_frame_time() <= budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_of_no_samples_is_zero() {
        let timer = FrameTimer::new(10);
        assert_eq!(timer.average_frame_time(), Duration::ZERO);
    }

    #[test]
    fn average_of_uniform_samples() {
        let mut timer = FrameTimer::new(10);
        for _ in 0..5 {
            timer.record(Duration::from_millis(8));
        }
        assert_eq!(timer.average_frame_time(), Duration::from_millis(8));
    }

    #[test]
    fn drops_oldest_sample_beyond_capacity() {
        let mut timer = FrameTimer::new(2);
        timer.record(Duration::from_millis(100)); // will be evicted
        timer.record(Duration::from_millis(8));
        timer.record(Duration::from_millis(8));
        assert_eq!(timer.average_frame_time(), Duration::from_millis(8));
    }

    #[test]
    fn fps_matches_average_frame_time() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_millis(10)); // 100fps
        assert!((timer.fps() - 100.0).abs() < 0.5);
    }

    #[test]
    fn meets_budget_true_when_under() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_micros(8000));
        assert!(timer.meets_budget(BUDGET_120FPS));
    }

    #[test]
    fn meets_budget_false_when_over() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_millis(16));
        assert!(!timer.meets_budget(BUDGET_120FPS));
    }
}
