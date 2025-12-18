use nexus::kernel::event::{Event, InputEvent, OutputStatus};
use nexus::kernel::reactor::Reactor;
use nexus::kernel::state::StateDelta;
use nexus::kernel::time::Tick;
use tokio::sync::mpsc;
use nexus::kernel::latent::{LatentSlot, Modality, LatentState};
use nexus::planner::types::{PlanningEpoch, Intent};

#[test]
fn test_phase6_0_uncertainty_math() {
    // Test 0: Explicit contract for Global Uncertainty Math
    // Logic: Uncertainty = (1.0 - AverageConfidence).max(0.0)
    // Empty -> 0.0 (Quiescent/Stable by default)
    
    // Case A: Empty
    let mut state = LatentState::default();
    assert_eq!(state.global_uncertainty(), 0.0, "Empty state should have 0 uncertainty (Quiescent)");
    
    // Case B: Single Low Confidence Slot (Confusion)
    state.slots.push(LatentSlot {
        values: vec![],
        confidence: 0.1,
        created_at: Tick { frame: 0 },
        modality: Modality::Audio,
        decay_rate: 0.0,
    });
    // Avg Conf = 0.1. Uncertainty = 0.9.
    assert!((state.global_uncertainty() - 0.9).abs() < f32::EPSILON, "Low conf should imply High Uncertainty");
    
    // Case C: Mixed Confidence (Conflict/Partial)
    state.slots.push(LatentSlot {
        values: vec![],
        confidence: 0.9,
        created_at: Tick { frame: 0 },
        modality: Modality::Visual,
        decay_rate: 0.0,
    });
    // Slots: 0.1, 0.9. Avg = 0.5. Uncertainty = 0.5.
    assert!((state.global_uncertainty() - 0.5).abs() < f32::EPSILON, "Mixed conf should average out");
    
    // Case D: High Confidence (Stable)
    state.slots.clear();
    state.slots.push(LatentSlot {
        values: vec![],
        confidence: 1.0,
        created_at: Tick { frame: 0 },
        modality: Modality::Visual,
        decay_rate: 0.0,
    });
    // Avg = 1.0. Uncertainty = 0.0.
    assert_eq!(state.global_uncertainty(), 0.0, "High confidence should imply 0 uncertainty");
}

#[tokio::test]
async fn test_phase6_1_gate_instability() {
    // Test 1: High Uncertainty -> Gate DENIES output
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Inject Low Confidence Latent (Uncertainty ~ 0.9)
    let slot = LatentSlot {
        values: vec![0.0],
        confidence: 0.1, // Very low confidence
        created_at: Tick { frame: 0 },
        modality: Modality::Audio,
        decay_rate: 0.1,
    };
    reactor.state.reduce(StateDelta::LatentUpdate { slot });
    
    // Propose Response
    let intent = Intent::BeginResponse { confidence: 0.9 }; // Planner is confident, but Gate checks State
    let epoch = PlanningEpoch { tick: reactor.tick, state_version: reactor.state.version };
    
    // Run Step
    reactor.tick_step(vec![Event::PlanProposed(epoch, intent)]);
    
    // Assert No Output Proposed (Gate Denied)
    assert!(reactor.state.active_outputs().is_empty(), "Gate should deny output due to instability");
}

#[tokio::test]
async fn test_phase6_2_interruption_retraction() {
    // Test 2: SoftCommit -> Interruption -> Retracted (Canceled)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Manually inject a SoftCommit output (simulating AllowedPartial)
    let output_id = nexus::kernel::event::OutputId { tick: 0, ordinal: 0 };
    let output = nexus::kernel::event::Output {
        id: output_id,
        content: "It seems...".to_string(),
        status: OutputStatus::SoftCommit,
        proposed_at: Tick::new(),
        committed_at: None,
        parent_id: Some("root_task".to_string()),
    };
    reactor.state.reduce(StateDelta::OutputProposed(output));
    
    // 2. Interrupt (STOP)
    let stop = Event::Input(InputEvent::text("User", "STOP"));
    reactor.tick_step(vec![stop]);
    
    // 3. Assert Canceled
    let out = reactor.state.active_outputs().get(&output_id).unwrap();
    assert_eq!(out.status, OutputStatus::Canceled, "SoftCommit should be canceled on interruption");
}

#[tokio::test]
async fn test_phase6_3_silence_preference() {
    // Test 3: Planner wants to speak, Gate says No (Silence)
    // Similar to Test 1 but explicitly checking for Silence
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Ensure UserSpeaking = true -> Gate Deny
    reactor.state.user_speaking = true;
    
    let intent = Intent::BeginResponse { confidence: 1.0 };
    let epoch = PlanningEpoch { tick: reactor.tick, state_version: reactor.state.version };
    
    reactor.tick_step(vec![Event::PlanProposed(epoch, intent)]);
    
    assert!(reactor.state.active_outputs().is_empty(), "Gate must deny when user is speaking");
}

#[tokio::test]
async fn test_phase6_4_hard_vs_soft() {
    // Test 4: Stable State -> HardCommit
    // Note: Depends on Gate Logic and SymbolicSnapshot extraction
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Inject Stable Latent (Vision)
    let slot = LatentSlot {
        values: vec![1.0],
        confidence: 0.95, // High confidence -> Low Uncertainty (0.05)
        created_at: Tick { frame: 0 },
        modality: Modality::Visual,
        decay_rate: 0.0,
    };
    reactor.state.reduce(StateDelta::LatentUpdate { slot });
    
    // Propose Response
    let intent = Intent::BeginResponse { confidence: 1.0 };
    let epoch = PlanningEpoch { tick: reactor.tick, state_version: reactor.state.version };
    
    reactor.tick_step(vec![Event::PlanProposed(epoch, intent)]);
    
    // Assert Output exists
    assert!(!reactor.state.active_outputs().is_empty(), "Stable state must yield output");
    let out = reactor.state.active_outputs().values().next().unwrap();
    
    // Verify HardCommit
    assert_eq!(out.status, OutputStatus::HardCommit, "Stable state should yield HardCommit");
    assert!(!out.content.starts_with("It seems"), "HardCommit should not hedge");
}

#[tokio::test]
async fn test_phase6_5_monotonic_commitment() {
    // Test 5: SoftCommit DOES NOT upgrade to HardCommit automatically
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Create SoftCommit Output
    let output_id = nexus::kernel::event::OutputId { tick: 0, ordinal: 0 };
    let output = nexus::kernel::event::Output {
        id: output_id,
        content: "It seems...".to_string(),
        status: OutputStatus::SoftCommit,
        proposed_at: Tick::new(),
        committed_at: None,
        parent_id: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output));
    
    // 2. Inject Stable Latent (State improves)
    let slot = LatentSlot {
        values: vec![1.0],
        confidence: 0.99,
        created_at: Tick { frame: 0 },
        modality: Modality::Visual,
        decay_rate: 0.0,
    };
    reactor.state.reduce(StateDelta::LatentUpdate { slot });
    
    // 3. Tick
    reactor.tick_step(vec![]);
    
    // 4. Assert Status is STILL SoftCommit
    let out = reactor.state.active_outputs().get(&output_id).unwrap();
    assert_eq!(out.status, OutputStatus::SoftCommit, "SoftCommit must NOT silent upgrade");
}
