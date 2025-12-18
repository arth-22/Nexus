use nexus::kernel::event::{Output, OutputId, OutputStatus, Event, InputEvent, InputContent};
use nexus::kernel::time::Tick;
use nexus::kernel::crystallizer::{self, CrystallizationDecision, SymbolicSnapshot, Claim};
use nexus::kernel::state::{SharedState, StateDelta, MetaLatents};
use nexus::monitor::{SelfObservationMonitor, SelfObservation};


// Helper to create a dummy input
fn create_text_input(content: &str) -> InputEvent {
    InputEvent {
        content: InputContent::Text(content.to_string()),
        source: "test".to_string(), 
    }
}

// Helper to create state with latents
fn create_state_with_uncertainty(uncertainty: f32) -> SharedState {
    let mut state = SharedState::new();
    // Inject latent slots to simulate uncertainty
    // global_uncertainty = 1.0 - (sum(confidence)/n) roughly or similar logic.
    // Let's look at latent.rs logic: uncertainty = 1.0 - mean_confidence?
    // We'll trust the `crystallizer` test logic implies we can manipulate it.
    // For this test, we can just MOCK the decision by manipulating the function arguments if possible,
    // but we are testing `check_gate`.
    // We need to inject a slot.
    
    use nexus::kernel::latent::{LatentSlot, Modality};
    state.latents.slots.push(LatentSlot {
        values: vec![0.0],
        confidence: 1.0 - uncertainty, // If uncertainty is 0.8, confidence is 0.2
        created_at: Tick { frame: 0 },
        modality: Modality::Text,
        decay_rate: 0.0,
    });
    
    state
}

#[test]
fn test_unprompted_correction_monitor() {
    // Test 1: Observation -> Penalty Increase
    let mut monitor = SelfObservationMonitor::new();
    let tick = 1;
    
    // Simulate "UserCorrection"
    let events = vec![SelfObservation::UserCorrection { output_id: None }];
    
    // Run tick
    let delta = monitor.tick(tick, &events).expect("Should emit delta");
    
    match delta {
        StateDelta::MetaLatentUpdate { delta } => {
            assert!(delta.confidence_penalty > 0.0, "Penalty should rise after correction");
            assert!(delta.correction_bias > 0.0, "Correction bias should rise");
            println!("Penalty: {}, Bias: {}", delta.confidence_penalty, delta.correction_bias);
        }
        _ => panic!("Wrong delta type"),
    }
}

#[test]
fn test_confidence_adjustment_gate() {
    // Test 2: High Penalty -> Gate Denies 
    let mut state = create_state_with_uncertainty(0.65); // Uncertainty 0.65
    // Default Deny is 0.8. Default Partial is 0.4.
    // With 0.65, normally it might be AllowPartial or Delay (if > 0.6).
    // Let's set it to exactly 0.7 to be sure it's barely allowed or delayed.
    
    state.latents.slots[0].confidence = 0.3; // Uncertainty = 0.7
    
    // Validate baseline: 0.7 < 0.8 (Deny). So it is NOT Denied by default logic (only > 0.8 is denied).
    // Crystallizer: if > 0.8 return Deny. if > 0.6 return Delay.
    // So baseline is Delay.
    
    let decision = crystallizer::check_gate(&state);
    // assert_eq!(decision, CrystallizationDecision::Delay { ms: 500 }); // Assuming logic
    
    // Now apply Penalty
    state.meta_latents.confidence_penalty = 1.0; // Max penalty
    // Effective Threshold = 0.8 - (1.0 * 0.3) = 0.5.
    // Uncertainty 0.7 > 0.5? Yes.
    // Expect: Deny.
    
    let decision_biased = crystallizer::check_gate(&state);
    assert_eq!(decision_biased, CrystallizationDecision::Deny, "High penalty should strictly deny moderate uncertainty");
}

#[test]
fn test_recovery_decay() {
    // Test 3: Decay works
    let mut monitor = SelfObservationMonitor::new();
    let mut tick = 0;
    
    // Spike the penalty
    let events = vec![SelfObservation::UserCorrection { output_id: None }];
    let _ = monitor.tick(tick, &events);
    
    // Get current state
    let d1 = monitor.tick(tick + 1, &[]); // Emit current state
    let start_penalty = if let Some(StateDelta::MetaLatentUpdate { delta }) = d1 { delta.confidence_penalty } else { 0.0 };
    assert!(start_penalty > 0.0);
    
    // Fast forward 100 ticks (10 seconds?)
    tick += 100; // 100 * 10ms? or if tick is 100ms...
    // Monitor logic: decay_factor = 0.01 * elapsed.
    // If elapsed = 100, factor = 1.0. Full decay?
    
    let d2 = monitor.tick(tick, &[]);
    let end_penalty = if let Some(StateDelta::MetaLatentUpdate { delta }) = d2 { delta.confidence_penalty } else { 0.0 };
    
    println!("Start: {}, End: {}", start_penalty, end_penalty);
    assert!(end_penalty < start_penalty, "Penalty must decay over time");
}

#[test]
fn test_silence_logic() {
    // Test 4: Silence (Deny) when confused + penalized
    // Same as Test 2 but conceptually distinct: verifies that adding penalty pushes "Partial" into "Deny".
    let mut state = create_state_with_uncertainty(0.45); // Uncertainty 0.45
    // Baseline: > 0.4 -> AllowPartial. Ref crystallizer.rs
    
    let decision = crystallizer::check_gate(&state);
    assert_eq!(decision, CrystallizationDecision::AllowPartial, "Baseline should allow partial");
    
    // Apply Penalty
    state.meta_latents.confidence_penalty = 1.0;
    // Effective Deny Threshold = 0.5. Input 0.45.
    // Wait, 0.45 < 0.5. So it shouldn't be Denied by the *modified* Deny threshold?
    // Crystallizer logic: if uncertainty > effective_deny_threshold { Deny }.
    // 0.45 is not > 0.5.
    // So it might still be AllowPartial.
    // Unless Partial Logic also shifts?
    // Implementation didn't shift Partial Threshold. 
    // This is correct behavior: we only strictly DENY high uncertainty stuff.
    // If it's 0.45 (pretty confident), we still allow it.
    // But let's try 0.55.
    
    let mut state2 = create_state_with_uncertainty(0.55);
    state2.meta_latents.confidence_penalty = 1.0;
    // 0.55 > 0.5 -> Deny.
    
    let decision2 = crystallizer::check_gate(&state2);
    assert_eq!(decision2, CrystallizationDecision::Deny, "Should silence moderate uncertainty when penalized");
}
