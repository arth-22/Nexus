use crate::kernel::state::SharedState;
use crate::kernel::time::Tick;
use crate::planner::types::Intent;

/// Step 6: Pure Planner.
/// (State, Tick) -> Vec<Intent>
pub fn plan(_state: &SharedState, _tick: Tick) -> Vec<Intent> {
    // Deterministic Logic Stub
    // Example: If it's the 10th frame, say hello.
    if _tick.frame == 10 {
        return vec![Intent::BeginResponse { confidence: 1.0 }];
    }
    
    vec![]
}
