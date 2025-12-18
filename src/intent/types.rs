use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::kernel::time::Tick;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentId(pub Uuid);

impl IntentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStatus {
    Active,
    Suspended,
    Dissolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongHorizonIntent {
    pub id: IntentId,
    /// Symbolic label (e.g. "PlanTrip"), not free text.
    pub description: String, 
    pub confidence: f32, // 0.0 - 1.0
    pub decay_rate: f32, // Per tick decay
    pub status: IntentStatus,
    pub created_at: Tick,
    pub last_reinforced: Tick,
}
