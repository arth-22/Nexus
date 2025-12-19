use nexus::kernel::reactor::Reactor;
use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal, AudioStatus};
use nexus::kernel::intent::types::{IntentState, IntentStability, IntentHypothesis, IntentCandidate};
use nexus::kernel::time::Tick;
use nexus::kernel::intent::long_horizon::{IntentStatus, LongHorizonIntent};
use nexus::kernel::state::StateDelta;
use tokio::sync::mpsc;
use std::collections::HashMap;

// Helper to inject a stable intent (which registers LongHorizon)
fn inject_stable_intent(reactor: &mut Reactor, text: &str, symbol_id: &str) {
    let cand = IntentCandidate {
        id: "cand1".to_string(),
        hypothesis: IntentHypothesis::Inquiry,
        confidence: 0.9,
        source_symbol_ids: vec![symbol_id.to_string()],
        semantic_hash: 12345, // Dummy
        stability: IntentStability::Stable,
    };
    // Direct reduction to simulate Phase G output
    reactor.state.reduce(StateDelta::AssessmentUpdate(IntentState::Stable(cand.clone())));
    
    // Trigger Reactor to register it
    // We can simulate the tick logic by calling register_intent directly OR running tick_step.
    // Reactor tick_step checks intent_state.
    // If we call tick_step with Empty input, it sees Stable -> Registers.
    // BUT Reactor logic is edge-triggered on ASSESSMENT UPDATE in Input processing loop.
    // Simply setting state via reduce won't trigger the "Input -> Assess -> Memory/Intent" flow in tick_step line 249.
    // We must invoke `lhim.register_intent` via reactor or simulate the Input event that causes stability.
    //
    // Let's use `lhim.register_intent` directly for setup speed, 
    // simulating what the Reactor would do.
    let deltas = reactor.lhim.register_intent(&cand, &reactor.state, reactor.tick);
    for d in deltas {
        reactor.state.reduce(d);
    }
}

#[tokio::test]
async fn test_interruption_preservation() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup Active Intent
    inject_stable_intent(&mut reactor, "Hello", "seg1");
    assert_eq!(reactor.state.active_intents.len(), 1);
    let id = reactor.state.active_intents.keys().next().unwrap().clone();
    assert_eq!(reactor.state.active_intents[&id].status, IntentStatus::Active);

    // 2. Interrupt with SpeechStart
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Test".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    })]);

    // 3. Assert Suspended
    assert_eq!(reactor.state.active_intents[&id].status, IntentStatus::Suspended, "Intent should safely suspend");
}

#[tokio::test]
async fn test_system_speaking_does_not_suspend() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup Active Intent
    inject_stable_intent(&mut reactor, "Hello", "seg1");
    let id = reactor.state.active_intents.keys().next().unwrap().clone();

    // 2. Inject System Speaking (PlaybackStarted)
    // This updates `audio_monitor.system_speaking` but should NOT trigger interruption in LHIM.
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "System".to_string(),
        content: InputContent::AudioStatus(AudioStatus::PlaybackStarted),
    })]);

    // 3. Assert Still Active
    assert_eq!(reactor.state.active_intents[&id].status, IntentStatus::Active, "System speaking should NOT suspend intent");
}

#[tokio::test]
async fn test_silent_resumption() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup Active Intent & Suspend it
    inject_stable_intent(&mut reactor, "Hello", "seg1");
    let id = reactor.state.active_intents.keys().next().unwrap().clone();
    
    // Manually suspend
    let susp_deltas = reactor.lhim.suspend_intent(&id, &reactor.state, reactor.tick).unwrap();
    reactor.state.reduce(susp_deltas);
    assert_eq!(reactor.state.active_intents[&id].status, IntentStatus::Suspended);

    // 2. Inject Context (Symbol "seg1" reappears in Forming state)
    // We simulate a ProvisionalText input which sets IntentState::Forming, 
    // but Reactor tick_step does that via Arbitrator.
    // We can manually set state.intent_state then run tick_step with Empty input.
    // Wait, tick_step logic checks lhim.try_resume().
    // try_resume checks state.intent_state.
    
    let forming_cand = IntentCandidate {
        id: "forming1".to_string(),
        hypothesis: IntentHypothesis::Inquiry,
        confidence: 0.5,
        source_symbol_ids: vec!["seg1".to_string()], // MATCH!
        semantic_hash: 0,
        stability: IntentStability::Unstable,
    };
    reactor.state.reduce(StateDelta::AssessmentUpdate(IntentState::Forming(vec![forming_cand])));
    
    // 3. Run Tick
    let side_effects = reactor.tick_step(vec![]);
    
    // 4. Assert Resumed
    assert_eq!(reactor.state.active_intents[&id].status, IntentStatus::Active, "Should resume on context match");
    // Assert NO Output (Silent) - SideEffects should be empty or unrelated
    assert!(side_effects.is_empty(), "Resumption should be silent");
}

#[tokio::test]
async fn test_decay_model() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup Active
    inject_stable_intent(&mut reactor, "Hello", "seg1");
    let id = reactor.state.active_intents.keys().next().unwrap().clone();
    
    // 2. Advance massive time
    // Logic: score *= rate^delta.
    // rate = 0.9997.
    // Threshold Dormant = 0.3.
    // 0.3 = 0.9997^ticks -> ticks ~ 4000.
    
    let jump = 5000;
    reactor.state.reduce(StateDelta::Tick(Tick { frame: jump })); 
    reactor.tick.frame = jump;
    
    // 3. Run Tick (Apply Decay)
    reactor.tick_step(vec![]);
    
    // 4. Assert Dormant
    let intent = &reactor.state.active_intents[&id];
    assert!(intent.decay_score < 0.3, "Score should be low: {}", intent.decay_score);
    assert_eq!(intent.status, IntentStatus::Dormant, "Should be Dormant");
}
