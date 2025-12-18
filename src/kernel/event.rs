use super::time::Tick;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    // Future: HighEnergySpike
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
    Committed,
    Canceled,
}
