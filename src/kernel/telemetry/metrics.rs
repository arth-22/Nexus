use std::collections::VecDeque;
use super::event::{TelemetryEvent, MemoryEventKind, DialogueActKind};
use crate::kernel::intent::long_horizon::IntentStatus;

#[derive(Debug, Clone, Default)]
pub struct TelemetrySnapshot {
    pub silence_stats: SilenceStats,
    pub interruption_stats: InterruptionStats,
    pub intent_stats: IntentStats,
    pub memory_stats: MemoryStats,
    pub dialogue_stats: DialogueStats,
}

#[derive(Debug, Clone, Default)]
pub struct SilenceStats {
    pub total_periods: u64,
    pub total_ticks: u64,
    pub avg_silence_ticks: f64,
    pub max_silence_ticks: u64,
}

#[derive(Debug, Clone, Default)]
pub struct InterruptionStats {
    pub count: u64,
    pub total_latency_ticks: u64,
    pub avg_cancel_latency_ticks: f64,
}

#[derive(Debug, Clone, Default)]
pub struct IntentStats {
    pub created: u64,
    pub suspended: u64,
    pub resumed: u64,
    pub invalidated: u64,
    pub total_dormant_ticks: u64,
    pub avg_dormancy_ticks: f64,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub candidates_created: u64,
    pub reinforced: u64,
    pub promoted: u64,
    pub decayed: u64,
    pub forgotten: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DialogueStats {
    pub clarifications: u64,
    pub confirmations: u64,
    pub offers: u64,
    pub silent_waits: u64,
}

pub fn compute_snapshot(events: &VecDeque<TelemetryEvent>) -> TelemetrySnapshot {
    let mut snap = TelemetrySnapshot::default();
    
    let mut silence_accum_count = 0;
    let mut resumption_count = 0;
    
    for event in events {
        match event {
            TelemetryEvent::SilencePeriod { duration_ticks } => {
                snap.silence_stats.total_periods += 1;
                snap.silence_stats.total_ticks += duration_ticks;
                if *duration_ticks > snap.silence_stats.max_silence_ticks {
                    snap.silence_stats.max_silence_ticks = *duration_ticks;
                }
                silence_accum_count += 1;
            }
            TelemetryEvent::Interruption { source: _, cancel_latency_ticks } => {
                snap.interruption_stats.count += 1;
                snap.interruption_stats.total_latency_ticks += cancel_latency_ticks;
            }
            TelemetryEvent::IntentLifecycle { to, .. } => {
                match to {
                    IntentStatus::Active => snap.intent_stats.created += 1, // Approximation: Active is creation or resumption? 
                    // To distinguish, we'd need 'from'. if from==None? But Lifecycle assumes non-optional.
                    // Actually, register_intent creates Active. resume makes Active.
                    // For now, count transitions.
                    IntentStatus::Suspended => snap.intent_stats.suspended += 1,
                    IntentStatus::Invalidated => snap.intent_stats.invalidated += 1,
                    _ => {}
                }
            }
            TelemetryEvent::IntentResumption { dormant_ticks, .. } => {
                snap.intent_stats.resumed += 1;
                snap.intent_stats.total_dormant_ticks += dormant_ticks;
                resumption_count += 1;
            }
            TelemetryEvent::MemoryEvent { kind, .. } => {
                match kind {
                    MemoryEventKind::CandidateCreated => snap.memory_stats.candidates_created += 1,
                    MemoryEventKind::Reinforced => snap.memory_stats.reinforced += 1,
                    MemoryEventKind::Promoted => snap.memory_stats.promoted += 1,
                    MemoryEventKind::Decayed => snap.memory_stats.decayed += 1,
                    MemoryEventKind::Forgotten => snap.memory_stats.forgotten += 1,
                    MemoryEventKind::AttributesUpdated => {} // Tracking only
                }
            }
            TelemetryEvent::DialogueAct { act } => {
                match act {
                    DialogueActKind::AskClarification => snap.dialogue_stats.clarifications += 1,
                    DialogueActKind::Confirm => snap.dialogue_stats.confirmations += 1,
                    DialogueActKind::Offer => snap.dialogue_stats.offers += 1,
                    DialogueActKind::Wait | DialogueActKind::StaySilent => snap.dialogue_stats.silent_waits += 1,
                }
            }
            _ => {}
        }
    }
    
    // Compute Averages
    if silence_accum_count > 0 {
        snap.silence_stats.avg_silence_ticks = snap.silence_stats.total_ticks as f64 / silence_accum_count as f64;
    }
    
    if snap.interruption_stats.count > 0 {
        snap.interruption_stats.avg_cancel_latency_ticks = snap.interruption_stats.total_latency_ticks as f64 / snap.interruption_stats.count as f64;
    }
    
    if resumption_count > 0 {
        snap.intent_stats.avg_dormancy_ticks = snap.intent_stats.total_dormant_ticks as f64 / resumption_count as f64;
    }
    
    snap
}
