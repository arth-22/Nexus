use serde::{Serialize, Deserialize};
use crate::kernel::time::Tick;
use crate::kernel::presence::PresenceState;
use crate::kernel::event::OutputId;
use crate::kernel::intent::types::DialogueAct; // We'll map to a sanitized kind
use crate::kernel::intent::long_horizon::{IntentId, IntentStatus};
use crate::kernel::memory::types::MemoryId;

// Allowed: IDs, Timestamps, Durations, Counts, Enums
// Forbidden: Text, Audio Frames, Embeddings, Confidence Scores (if derived from content)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryEvent {
    PresenceTransition {
        from: PresenceState,
        to: PresenceState,
        tick: Tick,
    },

    SilencePeriod {
        duration_ticks: u64,
    },

    OutputLifecycle {
        output_id: OutputId,
        event: OutputEventKind,
        latency_ticks: u64, // e.g. Time since request? Or just event time
    },

    Interruption {
        source: InterruptionSource,
        cancel_latency_ticks: u64, // Time from Interruption Signal to Output Cancellation
    },

    IntentLifecycle {
        intent_id: IntentId,
        from: IntentStatus,
        to: IntentStatus,
    },

    IntentResumption {
        intent_id: IntentId,
        dormant_ticks: u64,
    },

    MemoryEvent {
        kind: MemoryEventKind,
        memory_id: MemoryId,
    },

    DialogueAct {
        act: DialogueActKind,
    },

    Lifecycle(LifecycleEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputEventKind {
    DraftStarted,
    HardCommit,
    SoftCommit,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptionSource {
    AudioSpeechStart,
    ExplicitCancel,
    NewIntentConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryEventKind {
    CandidateCreated,
    Reinforced,
    Promoted,
    Decayed,
    Forgotten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogueActKind {
    AskClarification,
    Confirm,
    Offer,
    Wait,
    StaySilent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEvent {
    OnboardingCompleted,
}

impl From<&DialogueAct> for DialogueActKind {
    fn from(act: &DialogueAct) -> Self {
        match act {
            DialogueAct::AskClarification(_) => DialogueActKind::AskClarification, // Content STRIPPED
            DialogueAct::Confirm(_) => DialogueActKind::Confirm,                   // Content STRIPPED
            DialogueAct::Offer(_) => DialogueActKind::Offer,                       // Content STRIPPED
            DialogueAct::Wait => DialogueActKind::Wait,
            DialogueAct::StaySilent => DialogueActKind::StaySilent,
        }
    }
}
