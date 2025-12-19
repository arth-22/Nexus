use std::collections::VecDeque;
use super::event::TelemetryEvent;
use super::metrics::{TelemetrySnapshot, compute_snapshot};

const MAX_EVENTS: usize = 10_000;

#[derive(Debug)]
pub struct TelemetryRecorder {
    buffer: VecDeque<TelemetryEvent>,
}

impl TelemetryRecorder {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(MAX_EVENTS),
        }
    }

    pub fn record(&mut self, event: TelemetryEvent) {
        if self.buffer.len() >= MAX_EVENTS {
            self.buffer.pop_front();
        }
        self.buffer.push_back(event);
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        // Delegate to pure functional metrics module
        compute_snapshot(&self.buffer)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
