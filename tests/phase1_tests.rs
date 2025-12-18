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
    
    println!("Test A Passed: Stale Intent Rejected");
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
