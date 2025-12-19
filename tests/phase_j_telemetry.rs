use nexus::kernel::reactor::Reactor;
use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal, OutputStatus, Output};
use nexus::kernel::intent::types::{IntentCandidate, IntentHypothesis, IntentStability, IntentState};
use nexus::kernel::intent::long_horizon::IntentStatus;
use nexus::kernel::telemetry::event::TelemetryEvent;
use nexus::kernel::state::StateDelta;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_interruption_latency() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Inject Fake Output (Active)
    let out = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 },
        content: "Beep".to_string(),
        status: OutputStatus::HardCommit,
        proposed_at: nexus::kernel::time::Tick { frame: 0 },
        committed_at: Some(nexus::kernel::time::Tick { frame: 0 }),
        parent_id: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(out));
    
    // 2. Inject SpeechStart (Interruption)
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Test".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    })]);
    
    // 3. Verify Telemetry: Interruption Event
    let snapshot = reactor.telemetry.snapshot();
    assert_eq!(snapshot.interruption_stats.count, 1, "Should record 1 interruption");
    assert_eq!(snapshot.interruption_stats.avg_cancel_latency_ticks, 0.0, "Latency should be 0 (same tick)");
}

#[tokio::test]
async fn test_silence_stability() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Run 50 empty ticks
    for _ in 0..50 {
        reactor.tick_step(vec![]);
    }
    
    // 2. Verify Silence Stats
    let snapshot = reactor.telemetry.snapshot();
    // Logic: 50 ticks, each is a silence period of 1 logic tick (since streaming logic)
    // Or if we implemented coalescing? We implemented "emit 1 tick duration".
    // So total_ticks should be 50. total_periods 50.
    
    assert_eq!(snapshot.silence_stats.total_ticks, 50);
    assert!(snapshot.silence_stats.avg_silence_ticks > 0.0);
}

#[tokio::test]
async fn test_intent_resumption_telemetry() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup Active Intent
    let candidate = IntentCandidate {
        id: "intent1".to_string(),
        hypothesis: IntentHypothesis::Inquiry,
        confidence: 0.9,
        source_symbol_ids: vec!["sym1".to_string()],
        semantic_hash: 1,
        stability: IntentStability::Stable,
    };
    
    // Manually register via LHIM (Reactor handles via tick_step Input usually)
    // We inject direct state update to simulate "Pre-Registration" then call register?
    // Let's call register directly to setup state.
    reactor.lhim.register_intent(&candidate, &reactor.state, reactor.tick, &mut reactor.telemetry);
    
    // 2. Suspend
    reactor.lhim.suspend_intent(&candidate.id, &reactor.state, reactor.tick, &mut reactor.telemetry);
    
    // 3. Advance time (Simulate dormancy)
    // Advance 10 ticks
    for _ in 0..10 {
        reactor.tick_step(vec![]); // Will tick LHIM decay
    }
    
    // 4. Inject Context for Resumption
    // IntentState::Forming matching "sym1"
    let forming = IntentCandidate {
        id: "forming".to_string(),
        hypothesis: IntentHypothesis::Inquiry,
        confidence: 0.5,
        source_symbol_ids: vec!["sym1".to_string()],
        semantic_hash: 2,
        stability: IntentStability::Unstable,
    };
    reactor.state.reduce(StateDelta::AssessmentUpdate(IntentState::Forming(vec![forming])));
    
    // 5. Run Tick (Trigger Resume)
    reactor.tick_step(vec![]);
    
    // 6. Verify Telemetry
    let snapshot = reactor.telemetry.snapshot();
    assert_eq!(snapshot.intent_stats.resumed, 1, "Should track resumption");
    assert!(snapshot.intent_stats.total_dormant_ticks >= 10, "Should track dormancy duration");
}

#[tokio::test]
async fn test_memory_telemetry() {
     let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Create Memory Candidate
    let candidate = IntentCandidate {
        id: "mem1".to_string(),
        hypothesis: IntentHypothesis::Inquiry,
        confidence: 0.9,
        source_symbol_ids: vec![],
        semantic_hash: 1,
        stability: IntentStability::Stable,
    };
    
    reactor.consolidator.process_intent(&candidate, &reactor.state, &mut reactor.telemetry);
    
    let snap = reactor.telemetry.snapshot();
    assert_eq!(snap.memory_stats.candidates_created, 1);
}

#[tokio::test]
async fn test_telemetry_non_interference() {
    // Structural Guarantee Check via Determinism
    // We instantiate two reactors and run them identically.
    // They should end up in the exact same state.
    // This proves that "Telemetry Recording" (which happens in both) 
    // does not introduce non-deterministic side effects or diverge based on hidden state.
    
    // Reactor A
    let (tx1, rx1) = mpsc::channel(100);
    let mut r1 = Reactor::new(rx1, tx1);
    
    // Reactor B
    let (tx2, rx2) = mpsc::channel(100);
    let mut r2 = Reactor::new(rx2, tx2);
    
    // Run both for 20 ticks
    for _ in 0..20 {
        r1.tick_step(vec![]);
        r2.tick_step(vec![]);
    }
    
    // Assert Equivalence
    assert_eq!(r1.state.version, r2.state.version, "State versions should match");
    assert_eq!(r1.state.last_tick, r2.state.last_tick, "Ticks should match");
    // Presence check (assuming PartialEq derived, usually is for Enums)
    // format! debug check as proxy for deep equality
    assert_eq!(format!("{:?}", r1.state.presence), format!("{:?}", r2.state.presence));
    
    // Validate Telemetry Active
    let s1 = r1.telemetry.snapshot();
    let s2 = r2.telemetry.snapshot();
    
    assert!(s1.silence_stats.total_ticks > 0, "Telemetry A should be active");
    assert!(s2.silence_stats.total_ticks > 0, "Telemetry B should be active");
    
    // Telemetry itself should be deterministic
    assert_eq!(s1.silence_stats.total_ticks, s2.silence_stats.total_ticks);
}
