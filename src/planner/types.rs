use serde::{Deserialize, Serialize};
use crate::kernel::time::Tick;

use crate::kernel::event::OutputId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanningEpoch {
    pub tick: Tick,
    pub state_version: u64, // Monotonic version counter
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "intent", content = "data")]
pub enum Intent {
    BeginResponse { confidence: f32 },
    Delay { ticks: u64 }, // Logical time, not wall clock
    AskClarification { context: String },
    ReviseStatement { ref_id: OutputId, correction: String },
    DoNothing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub epoch: PlanningEpoch,
    pub last_input_ticks: u64,
    pub user_active: bool,
    pub active_outputs: usize,
    pub recent_interruptions: usize,
    pub latent_summary: String, // Textual firewall for planner
    pub meta_mood: String, // "Cautious", "Confident", etc.
    pub intent_context: crate::kernel::intent::long_horizon::IntentContext,
}
