use nexus::kernel::state::{SharedState, StateDelta};
use nexus::kernel::time::Tick;
use nexus::intent::{LongHorizonIntentManager, IntentStatus, IntentId};
use std::collections::HashMap;

// Helper to create state with an active intent
fn create_state_with_intent(lhim: &LongHorizonIntentManager) -> (SharedState, IntentId) {
    let mut state = SharedState::new();
    let tick = Tick { frame: 0 };
    
    // Manual register via delta
    let delta = lhim.register_goal("TestTrip".to_string(), tick);
    if let StateDelta::IntentUpdate { intent } = &delta {
        let id = intent.id;
        state.reduce(delta);
        (state, id)
    } else {
        panic!("Failed to register goal");
    }
}

#[test]
fn test_graceful_resumption() {
    let lhim = LongHorizonIntentManager::new();
    let (mut state, id) = create_state_with_intent(&lhim);
    let mut tick = Tick { frame: 10 };
    
    // 1. Interrupt!
    let deltas = lhim.handle_interruption(&state);
    for d in deltas { state.reduce(d); }
    
    // Verify Suspended
    let intent = state.active_intents.get(&id).unwrap();
    assert_eq!(intent.status, IntentStatus::Suspended, "Interruption must suspend intent");
    
    // 2. Reinforce (Resume)
    if let Some(delta) = lhim.handle_reinforcement(id, &state, tick) {
        state.reduce(delta);
    }
    
    // Verify Active
    let intent = state.active_intents.get(&id).unwrap();
    assert_eq!(intent.status, IntentStatus::Active, "Reinforcement must resume intent");
}

#[test]
fn test_abandonment_decay() {
    let lhim = LongHorizonIntentManager::new();
    let (mut state, id) = create_state_with_intent(&lhim);
    
    // Initial confidence ~0.5
    let start_conf = state.active_intents.get(&id).unwrap().confidence;
    
    // Run 50 ticks of decay
    for i in 0..50 {
        let t = Tick { frame: 10 + i };
        let deltas = lhim.tick(t, &state);
        for d in deltas { state.reduce(d); }
    }
    
    let end_conf = state.active_intents.get(&id).unwrap().confidence;
    assert!(end_conf < start_conf, "Confidence must decay over time without reinforcement");
    
    // Run until dissolved (approx 200 ticks at 1% rate)
    for i in 0..300 {
         let t = Tick { frame: 60 + i };
        let deltas = lhim.tick(t, &state);
        for d in deltas { state.reduce(d); }
    }
    
    let intent = state.active_intents.get(&id).unwrap();
    assert_eq!(intent.status, IntentStatus::Dissolved, "Intent must dissolve when confidence drops low enough");
}

#[test]
fn test_interruption_penalty() {
    let lhim = LongHorizonIntentManager::new();
    let (mut state, id) = create_state_with_intent(&lhim);
    let start_conf = state.active_intents.get(&id).unwrap().confidence;

    // Interrupt
    let deltas = lhim.handle_interruption(&state);
    for d in deltas { state.reduce(d); }
    
    let after_interruption = state.active_intents.get(&id).unwrap();
    assert_eq!(after_interruption.status, IntentStatus::Suspended);
    assert!(after_interruption.confidence < start_conf, "Interruption must apply penalty");
}

#[test]
fn test_no_agentic_drift() {
    // Verify that LHIM outputs ONLY StateDelta::IntentUpdate, nothing else.
    // This is structurally guaranteed by the return types of LHIM methods, 
    // but we can verify that the Manager doesn't return `BeginResponse` or similar if we were mocking it.
    // Here we strictly check that `get_context` is pure data.
    
    let lhim = LongHorizonIntentManager::new();
    let (mut state, id) = create_state_with_intent(&lhim);
    
    let context = lhim.get_context(&state);
    assert_eq!(context.active_focus, Some("TestTrip".to_string()));
    
    // Now interrupt
    let deltas = lhim.handle_interruption(&state);
    for d in deltas { state.reduce(d); }
    
    // Context should be None/Weak if suspended?
    // Implementation: "active_intents.filter(Active)"
    // So Suspended intents do NOT show up in context! Excellent.
    let context_after = lhim.get_context(&state);
    assert!(context_after.active_focus.is_none(), "Suspended intents must NOT bias the planner");
}
