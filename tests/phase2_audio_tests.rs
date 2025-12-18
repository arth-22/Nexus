use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal, Output};
use nexus::kernel::reactor::Reactor;
use nexus::kernel::state::StateDelta;
use nexus::kernel::time::Tick;
use tokio::sync::mpsc;
use nexus::planner::types::{PlanningEpoch, Intent};

#[tokio::test]
async fn test_phase2_1_hard_interruption() {
    // Test 1: System speaking -> Audio SpeechStart -> Immediate Cancellation
    let (tx, rx) = mpsc::channel(100);
    // We send a clone to reactor, keep one for "planner" simulation if needed (not used here)
    let mut reactor = Reactor::new(rx, tx.clone());
    
    // 1. Setup: System is speaking
    // Manually inject an active output into state
    let output = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 }, // FIXED: u64
        parent_id: Some("root_task".to_string()),
        content: "Speaking...".into(),
        status: nexus::kernel::event::OutputStatus::Draft, // FIXED: Draft
        proposed_at: Tick::new(),
        committed_at: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output.clone()));
    
    // Verify State has active output
    assert!(!reactor.state.active_outputs().is_empty());
    
    // 2. Inject Audio SpeechStart (Tick 1)
    let audio_event = Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    });
    
    // Run Tick
    let effects = reactor.tick_step(vec![audio_event]);
    
    // Assert: No side effects (Interruption shouldn't spawn new things immediately, just cancel)
    // Actually, planner abort is internal. Output cancellation is internal state. 
    // SideEffects (like spawning audio) shouldn't happen if we just interrupted.
    assert!(effects.is_empty(), "Interruption should not generate side effects");
    
    // 3. Assertions
    // A) Cancellation happened? (Statuses updated to Canceled)
    let outputs = reactor.state.active_outputs();
    let out = outputs.get(&output.id).unwrap();
    assert_eq!(out.status, nexus::kernel::event::OutputStatus::Canceled, "Output should be canceled by SpeechStart");
    
    // B) User Speaking State?
    assert!(reactor.state.user_speaking, "User should be marked speaking");
}

#[tokio::test]
async fn test_phase2_2_turn_pressure_dynamics() {
    // Test 2: Turn Pressure Logic (Growth/Decay)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);

    // Initial Pressure 0.0
    assert_eq!(reactor.state.turn_pressure, 0.0);
    
    // 1. User Speaks while Quiescent (No system output) -> Pressure should NOT spike?
    // Plan said: "Increases if User Speaks AND active_outputs > 0"
    
    // Use Event to set speaking state naturally
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    })]);
    assert!(reactor.state.user_speaking);
    
    // Tick generated logic (StateDelta::Tick) ran in the above step too.
    
    // Pressure should remain 0 or low
    assert_eq!(reactor.state.turn_pressure, 0.0);

    // 2. Simulate System Output + User Speaking
    let output = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 }, // FIXED: u64
        parent_id: Some("root_task".to_string()),
        content: "Speaking...".into(),
        status: nexus::kernel::event::OutputStatus::Draft, // FIXED: Draft
        proposed_at: Tick::new(),
        committed_at: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output));
    
    // Tick with User Speaking + Active Output
    reactor.tick_step(vec![]);
    
    // Assert Pressure Growth
    assert!(reactor.state.turn_pressure > 0.0, "Pressure should increase when interrupting");
    assert_eq!(reactor.state.turn_pressure, 0.1);

    // 3. User Stops Speaking -> Pressure Decay
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechEnd),
    })]);
    
    // Physics runs effectively Pre-Input context of tick. 
    // Input changes state at END of this tick.
    // Tick 3 (SpeechEnd): Start=Speaking -> Pressure 0.1 -> 0.2. End=NotSpeaking.
    // Tick 4 (Empty): Start=NotSpeaking -> Pressure 0.2 -> 0.19.
    reactor.tick_step(vec![]);
    
    // Should decay by 0.01 per tick
    assert!(reactor.state.turn_pressure < 0.2, "Pressure should decay from peak");
    assert_eq!(reactor.state.turn_pressure, 0.19);
}

#[tokio::test]
async fn test_phase2_3_hesitation_derivation() {
    // Test 3: Hesitation (Short burst detection)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Tick 0: Speech Start
    // Need to set last_tick to 0 implicitly by new()
    let start_event = Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    });
    reactor.tick_step(vec![start_event]);
    assert!(reactor.state.user_speaking);
    assert_eq!(reactor.state.last_speech_start.unwrap().frame, 1); // Tick passed locally in tick_step before reduce? 
    
    // 2. Advance short time (e.g. 2 ticks = 40ms)
    reactor.tick_step(vec![]); // Tick 2
    reactor.tick_step(vec![]); // Tick 3
    
    // 3. Speech End (Tick 3)
    let end_event = Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechEnd),
    });
    reactor.tick_step(vec![end_event]); // Tick 4 processing
    
    // Duration: End(4) - Start(1) = 3 ticks (< 10 threshold)
    assert!(reactor.state.hesitation_detected, "Short burst should trigger hesitation");
    
    // 4. Long Speech Test
    // Reset
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    })]); // Start at Tick 5
    
    // Advance > 10 ticks
    for _ in 0..15 {
        reactor.tick_step(vec![]); 
    }
    
    // End
    reactor.tick_step(vec![Event::Input(InputEvent {
        source: "Audio".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechEnd),
    })]);
    
    assert!(!reactor.state.hesitation_detected, "Long speech should NOT trigger hesitation");
}
