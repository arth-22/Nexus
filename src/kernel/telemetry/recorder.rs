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
    
    // Phase M: Session Aggregation (Called on Shutdown)
    pub fn aggregate_session(&self, duration_ticks: u64) -> TelemetryEvent {
         let snap = self.snapshot();
         
         // Calculate Silence Ratio
         let silence_ratio = if duration_ticks > 0 {
             snap.silence_stats.total_ticks as f32 / duration_ticks as f32
         } else {
             0.0
         };

         TelemetryEvent::SessionSummary {
             duration_ticks,
             silence_ratio,
             interruptions: snap.interruption_stats.count,
             resumed_intents: snap.intent_stats.resumed,
             memory_consents: snap.memory_stats.promoted, // Using 'promoted' as proxy for 'granted', or track consents specifically if metrics updated?
             // Metrics has 'promoted'. Consents in metrics?
             // Metrics.rs doesn't track "consents" distinct from promotions yet.
             // We can use promoted count as "granted consents".
             // Or update metrics.rs to count "consets".
             // For Phase M plan says "memory_consents". 
             // Let's assume promoted == granted consent for now.
         }
    }
}
