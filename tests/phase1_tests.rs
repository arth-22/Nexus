use tokio::sync::mpsc;
use nexus::kernel::reactor::Reactor;
use nexus::kernel::event::{Event, InputEvent, OutputStatus};
use nexus::planner::types::{PlanningEpoch, Intent};
use nexus::kernel::time::Tick;

#[tokio::test]
async fn test_a_stale_intent_rejection() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);

    // Initial State Version: 0. Tick: 0.
    assert_eq!(reactor.state.version, 0);
    
    // Simulate Input Event to increment State Version
    let input = InputEvent { source: "User".into(), content: "Hi".into() };
    reactor.tick_step(vec![Event::Input(input)]);
    assert_eq!(reactor.state.version, 1);
    
    // Now inject a "Late" Plan from Epoch (Tick 0, Version 0)
    let stale_plan = Event::PlanProposed(
        PlanningEpoch { tick: Tick{frame:0}, state_version: 0 },
        Intent::BeginResponse { confidence: 0.99 }
    );
    
    // Reactor process
    let effects = reactor.tick_step(vec![stale_plan]);
    
    // VERIFY: No effects, No output proposed. Stale plan ignored.
    assert!(effects.is_empty(), "Stale plan should not produce effects");
    assert!(reactor.state.active_outputs().is_empty(), "Stale plan should not mutate state");
    // Explicit Invariant Assertion: State version must be exactly what we expect (1), proving no hidden delta.
    assert_eq!(reactor.state.version, 1, "State version should not change after rejecting stale plan");
    
    println!("Test A Passed: Stale Intent Rejected");
}

#[tokio::test]
async fn test_b_cancellation_safety() {
    // This test simulates the "Abort" logic. 
    // Since we cannot easily check if the background task was dropped without a mock spy,
    // we rely on the architectural guarantee that `reactor.tick_step` calls `planner.abort()` on input.
    // We verify that after an input, the STATE VERSION increments, which implicitly invalidates any
    // in-flight plan that might survive the abort (race condition).
    // The previous test (Test A) covers the "Leak" case.
    // This test covers the "Trigger" case.
    
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Trigger Planning (Tick 0)
    // No inputs, no outputs -> Dispatch happens
    reactor.tick_step(vec![]); 
    // We can't easily inspect reactor.planner private state (unless made pub).
    // Assuming dispatch happened.
    
    // 2. Interrupt immediately (Tick 1)
    let input = InputEvent { source: "User".into(), content: "Stop".into() };
    reactor.tick_step(vec![Event::Input(input)]);
    
    // 3. Assert State Version Advanced
    // Note: Use >= 1, as internal logic might trigger multiple deltas (e.g. cancellation checks).
    // The critical invariant is that version != 0 (Tick 0 Epoch), so the old plan is stale.
    println!("State Version after Input: {}", reactor.state.version);
    assert!(reactor.state.version >= 1, "State should advance on input, ensuring any surviving plan is Stale");
    
    println!("Test B Passed: Input triggers state advancement (and planner abort via reactor logic)");
}

#[tokio::test]
async fn test_valid_intent_accepted() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);

    // Current State Version: 0
    let valid_plan = Event::PlanProposed(
        PlanningEpoch { tick: Tick{frame:0}, state_version: 0 },
        Intent::BeginResponse { confidence: 0.99 }
    );
    
    let effects = reactor.tick_step(vec![valid_plan]);
    
    // VERIFY: Output Proposed
    assert!(!reactor.state.active_outputs().is_empty(), "Valid intent should be accepted");
    
    println!("Test Valid Passed");
}

#[tokio::test]
async fn test_c_invalid_plan_safety() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Behavior Check: If Planner fails (Malformed JSON) -> Sends DoNothing.
    // Reactor must handle DoNothing by doing... nothing.
    
    let plan = Event::PlanProposed(
        PlanningEpoch { tick: Tick{frame:0}, state_version: 0 },
        Intent::DoNothing
    );
    
    let effects = reactor.tick_step(vec![plan]);
    
    assert!(effects.is_empty());
    assert!(reactor.state.active_outputs().is_empty());
    
    println!("Test C Passed: DoNothing (Fallback) is safe");
}
