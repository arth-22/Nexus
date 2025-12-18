use nexus::kernel::event::{Event, InputEvent, InputContent, VisualSignal, Output};
use nexus::kernel::reactor::Reactor;
use nexus::kernel::state::StateDelta;
use nexus::kernel::time::Tick;
use tokio::sync::mpsc;
// use nexus::planner::types::{PlanningEpoch, Intent}; // If needed

#[tokio::test]
async fn test_phase3_1_context_shift_interruption() {
    // Test 2: Context Shift Interrupt (Hash change cancels output)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Setup: System is speaking/displaying
    let output = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 },
        parent_id: Some("root_task".to_string()),
        content: "Explaining previous visual...".into(),
        status: nexus::kernel::event::OutputStatus::Draft,
        proposed_at: Tick::new(),
        committed_at: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output.clone()));
    
    // 2. Inject Context Shift (Distance = 10 >> Threshold 5)
    let update_event = Event::Input(InputEvent {
        source: "Vision".to_string(),
        content: InputContent::Visual(VisualSignal::PerceptUpdate {
            hash: 0xDEADBEEF,
            distance: 10,
        }),
    });
    
    // Run Tick
    let _ = reactor.tick_step(vec![update_event]);
    
    // 3. Assert Cancellation
    let outputs = reactor.state.active_outputs();
    let out = outputs.get(&output.id).unwrap();
    assert_eq!(out.status, nexus::kernel::event::OutputStatus::Canceled, "Output should be canceled by Context Shift");
    
    // 4. Checking Stability Physics
    // Start 0.0 -> Distance 10 (Bad) -> (0.0 - 0.3).max(0.0) = 0.0
    // If it was already high, it should drop.
}

#[tokio::test]
async fn test_phase3_2_stability_dynamics() {
    // Test 3: Stability Gating (Unstable hash prevention)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Stable Inputs
    // Inject "Good" update (Distance 0 < 5)
    for _ in 0..10 {
        let update = Event::Input(InputEvent {
            source: "Vision".to_string(),
            content: InputContent::Visual(VisualSignal::PerceptUpdate {
                hash: 0xABC,
                distance: 0,
            }),
        });
        reactor.tick_step(vec![update]);
    }
    
    // Physics: Decay (-0.01) happens BEFORE Boost (+0.1) in each tick.
    // Net per tick = +0.09.
    // 10 ticks = 0.9.
    assert!(reactor.state.visual.stability_score >= 0.9, "Stability should rise to ~0.9");
    
    // 2. Unstable Input
    let score_before = reactor.state.visual.stability_score;
    
    // Inject "Bad" update (Distance 10 > 5)
    let bad_update = Event::Input(InputEvent {
        source: "Vision".to_string(),
        content: InputContent::Visual(VisualSignal::PerceptUpdate {
            hash: 0xDEF,
            distance: 10,
        }),
    });
    reactor.tick_step(vec![bad_update]);
    
    let score_after = reactor.state.visual.stability_score;
    
    // Should drop by 0.3 (penalty) + 0.01 (decay) = ~0.31
    let drop = score_before - score_after;
    assert!(drop >= 0.3, "Stability should drop by at least 0.3 (Penalty)");
    assert!(drop < 0.4, "Stability shouldn't drop too much");
    
    // 3. Silence Decay
    let score_after_drop = reactor.state.visual.stability_score;
    // Advance tick without update
    reactor.tick_step(vec![]);
    // Should drop by 0.01
    let score_final = reactor.state.visual.stability_score;
    
    assert!(score_final < score_after_drop, "Stability should have decayed");
    assert!((score_after_drop - score_final - 0.01).abs() < 0.001, "Decay should be exactly 0.01");
}

#[tokio::test]
async fn test_phase3_3_no_hallucination_on_silence() {
    // Test 4: Visual Silence (No hallucination)
    // Vision capture failure results in "No Event".
    // System should NOT invent a context change.
    
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Explicitly set hash to something
    reactor.state.visual.hash = 0x123;
    
    // Run empty ticks
    reactor.tick_step(vec![]);
    
    // Validate Hash unchanging
    assert_eq!(reactor.state.visual.hash, 0x123);
    
    // Validate no cancellation signal (implicit: active outputs remain active)
    let output = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 },
        parent_id: None,
        content: "Foo".into(),
        status: nexus::kernel::event::OutputStatus::Draft,
        proposed_at: Tick::new(),
        committed_at: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output.clone()));
    
    reactor.tick_step(vec![]);
    
    let outputs = reactor.state.active_outputs();
    let out = outputs.get(&output.id).unwrap();
    assert_eq!(out.status, nexus::kernel::event::OutputStatus::Draft);
}
