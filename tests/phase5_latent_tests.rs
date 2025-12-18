use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal, VisualSignal, Output};
use nexus::kernel::reactor::Reactor;
use nexus::kernel::state::StateDelta;
use nexus::kernel::time::Tick;
use tokio::sync::mpsc;
use nexus::kernel::latent::{LatentSlot, Modality};

#[tokio::test]
async fn test_phase5_1_decay_physics() {
    // Test 1: Decay Physics
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Manually inject a Latent Slot
    let slot = LatentSlot {
        values: vec![1.0],
        confidence: 1.0, 
        created_at: Tick { frame: 0 },
        modality: Modality::Audio,
        decay_rate: 0.1, // Exp(-0.1) approx 0.90 per tick
    };
    reactor.state.reduce(StateDelta::LatentUpdate { slot });
    
    // Tick 1
    // run tick_step with no inputs to drive time
    reactor.tick_step(vec![]);
    
    // Assert Confidence Drop
    let slot = &reactor.state.latents.slots[0];
    assert!(slot.confidence < 1.0, "Confidence should decay");
    assert!(slot.confidence > 0.8, "Confidence should be around 0.9");
    
    // Run 10 ticks
    for _ in 0..10 {
        reactor.tick_step(vec![]);
    }
    
    // Should be significantly lower
    // 0.9 ^ 10 ~ 0.34
    if let Some(slot) = reactor.state.latents.slots.first() {
         assert!(slot.confidence < 0.5, "Confidence should decay exponentially");
    } else {
         // It might be pruned if < 0.05. But 0.35 > 0.05.
    }
}

#[tokio::test]
async fn test_phase5_2_modality_separation() {
    // Test 2: Audio (Fast) vs Vision (Slow)
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Inject Audio
    reactor.tick_step(vec![Event::Input(InputEvent { 
        source: "Audio".into(), 
        content: InputContent::Audio(AudioSignal::SpeechStart) 
    })]);
    
    // Inject Vision
    reactor.tick_step(vec![Event::Input(InputEvent { 
        source: "Vision".into(), 
        content: InputContent::Visual(VisualSignal::PerceptUpdate { hash: 123, distance: 0 }) 
    })]);
    
    // Get slots
    let audio_conf = reactor.state.latents.slots.iter().find(|s| s.modality == Modality::Audio).unwrap().confidence;
    let visual_conf = reactor.state.latents.slots.iter().find(|s| s.modality == Modality::Visual).unwrap().confidence;
    
    // Run 5 ticks
    for _ in 0..5 {
        reactor.tick_step(vec![]);
    }
    
    let audio_conf_after = reactor.state.latents.slots.iter().find(|s| s.modality == Modality::Audio).unwrap().confidence;
    let visual_conf_after = reactor.state.latents.slots.iter().find(|s| s.modality == Modality::Visual).unwrap().confidence;
    
    let audio_decay = audio_conf - audio_conf_after;
    let visual_decay = visual_conf - visual_conf_after;
    
    assert!(audio_decay > visual_decay, "Audio should decay faster than Vision");
}

#[tokio::test]
async fn test_phase5_3_invariant_latent_non_authority() {
    // Test 3: Latents cannot override STOP
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // 1. Proposed Output
    let output = Output {
        id: nexus::kernel::event::OutputId { tick: 0, ordinal: 0 },
        parent_id: Some("root_task".to_string()),
        content: "Speaking...".into(),
        status: nexus::kernel::event::OutputStatus::Draft,
        proposed_at: Tick::new(),
        committed_at: None,
    };
    reactor.state.reduce(StateDelta::OutputProposed(output.clone()));
    
    // 2. Inject STRONG Latent (simulating high confidence "Continue")
    // (This acts as the bias)
    let slot = LatentSlot {
        values: vec![1.0], 
        confidence: 1.0, 
        created_at: Tick { frame: 0 },
        modality: Modality::Text, // Text intent
        decay_rate: 0.0, // No decay
    };
    reactor.state.reduce(StateDelta::LatentUpdate { slot });
    
    // 3. Inject STOP Command (Hard Control)
    let stop_input = Event::Input(InputEvent::text("User", "STOP"));
    
    // Run Tick
    reactor.tick_step(vec![stop_input]);
    
    // 4. Assert Cancellation
    let out = reactor.state.active_outputs().get(&output.id).unwrap();
    assert_eq!(out.status, nexus::kernel::event::OutputStatus::Canceled, "STOP must override Latents");
    
    // Belt-and-suspenders: Ensure no NEW outputs were proposed by Latent bias
    assert_eq!(reactor.state.active_outputs().len(), 1, "No new outputs should be proposed after STOP");
}

#[tokio::test]
async fn test_phase5_4_snapshot_integration() {
    // Test 4: Snapshot contains latent summary
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx);
    
    // Inject Audio
    reactor.tick_step(vec![Event::Input(InputEvent { 
        source: "Audio".into(), 
        content: InputContent::Audio(AudioSignal::SpeechStart) 
    })]);
    
    let snapshot = reactor.state.snapshot(reactor.tick);
    println!("Snapshot Summary: {}", snapshot.latent_summary);
    
    assert!(snapshot.latent_summary.contains("Audio"), "Snapshot must mention Audio");
    assert!(snapshot.latent_summary.contains("Conf"), "Snapshot must mention Confidence");
}
