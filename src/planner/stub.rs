use crate::kernel::state::SharedState;
use crate::kernel::time::Tick;
use crate::kernel::event::Intent;

/// Step 6: Pure Planner.
/// (State, Tick) -> Vec<Intent>
pub fn plan(_state: &SharedState, _tick: Tick) -> Vec<Intent> {
    // Deterministic Logic Stub
    // Example: If it's the 10th frame, say hello.
    if _tick.frame == 10 {
        return vec![Intent::Say { text: "Hello from Tick 10".to_string() }];
    }
    
    vec![]
}
