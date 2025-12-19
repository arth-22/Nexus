use super::time::Tick;
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputId {
    pub tick: u64,
    pub ordinal: u16,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// External signals (Audio, Text, System Signals)
    Input(InputEvent),
    PlanProposed(crate::planner::types::PlanningEpoch, crate::planner::types::Intent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSignal {
    SpeechStart,
    SpeechEnd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualSignal {
    /// Fact: A new percept has arrived.
    PerceptUpdate {
        hash: u64,
        distance: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioStatus {
    PlaybackStarted,
    PlaybackEnded, // Normalized: Finished OR Cancelled
}

#[derive(Debug, Clone)]
pub struct InputEvent {
    pub source: String,
    pub content: InputContent,
}

#[derive(Debug, Clone)]
pub enum InputContent {
    Text(String),
    Audio(AudioSignal),
    AudioChunk(Vec<f32>), // Raw audio frames from shell
    Visual(VisualSignal),
    ProvisionalText {
        content: String,
        confidence: f32,
        source_id: String, // SegmentId
    },
    TranscriptionRequest {
        segment_id: String, // Explicit gating trigger
    },
    AudioStatus(AudioStatus),
    // Phase L: Memory Consent
    MemoryConsentResponse {
        key: crate::kernel::memory::types::MemoryKey,
        state: crate::kernel::memory::consent::MemoryConsentState,
    },
}

// Helper for legacy text compatibility
impl InputEvent {
    pub fn text(source: &str, text: &str) -> Self {
        Self {
            source: source.to_string(),
            content: InputContent::Text(text.to_string()),
        }
    }
}

/// Outputs have a lifecycle managed by the Kernel
#[derive(Debug, Clone)]
pub struct Output {
    pub id: OutputId,
    pub content: String,
    pub status: OutputStatus,
    pub proposed_at: Tick,
    pub committed_at: Option<Tick>,
    pub parent_id: Option<String>, // UUID of parent task
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputStatus {
    Draft,
    SoftCommit, // Retractable
    HardCommit, // Durable
    Canceled,
    Committed, // Legacy/Finalized
}
