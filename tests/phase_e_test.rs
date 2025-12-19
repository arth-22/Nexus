use tokio::sync::mpsc;
use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal};
use nexus::kernel::reactor::Reactor;
use std::time::Duration;

#[tokio::test]
async fn test_phase_e_gated_asr() {
    // 1. Setup Reactor
    let (tx, rx) = mpsc::channel(100);
    // Use a separate channel for driver loop to avoid stealing events? 
    // Actually Reactor::new takes rx and tx.
    // We need to spawn the reactor in a background task to process events.
    
    let (tx_in, rx_in) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx_in, tx.clone());
    
    // Spawn Reactor Driver (Mocked Run Loop)
    // We can't easily run the full `reactor.run()` because it has infinite loop and SideEffects.
    // But we CAN use `tick_step`.
    
    // === Test 1: Buffering & Gate Closed ===
    println!("Step 1: Check Buffering & Gate Closed");
    
    // Simulate Speech Start (High Energy)
    // We bypass audio_monitor and inject VAD signals directly for precision
    let start_evt = Event::Input(InputEvent {
        source: "Test".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechStart),
    });
    
    let effects = reactor.tick_step(vec![start_evt]);
    assert!(reactor.state.active_segment_id.is_some(), "Should have active segment after SpeechStart");
    let initial_seg_id = reactor.state.active_segment_id.clone().unwrap();

    // Simulate Audio Chunks
    let chunk_evt = Event::Input(InputEvent {
        source: "Test".to_string(),
        content: InputContent::AudioChunk(vec![0.5; 480]), // 10ms chunk
    });
    let _ = reactor.tick_step(vec![chunk_evt]);
    
    // Verify frames appended
    let seg = reactor.state.audio_segments.get(&initial_seg_id).unwrap();
    assert_eq!(seg.frames.len(), 480, "Should buffer frames");

    // Simulate Speech End
    let end_evt = Event::Input(InputEvent {
        source: "Test".to_string(),
        content: InputContent::Audio(AudioSignal::SpeechEnd),
    });
    let effects_end = reactor.tick_step(vec![end_evt]);
    
    // Verify Segment Finalized
    assert!(reactor.state.active_segment_id.is_none(), "Active segment should be cleared");
    let seg_final = reactor.state.audio_segments.get(&initial_seg_id).unwrap();
    assert_eq!(seg_final.status, nexus::kernel::audio::segment::SegmentStatus::Pending, "Segment should be Pending");
    
    // Verify NO Transcription SideEffect (Gate Closed)
    let has_transcription = effects_end.iter().any(|e| matches!(e, nexus::kernel::scheduler::SideEffect::RequestTranscription { .. }));
    assert!(!has_transcription, "Should NOT request transcription automatically");

    // === Test 2: Gate Open (Explicit Request) ===
    println!("Step 2: Check Gate Open");
    
    let request_evt = Event::Input(InputEvent {
        source: "Planner".to_string(),
        content: InputContent::TranscriptionRequest { segment_id: initial_seg_id.clone() },
    });
    
    let effects_req = reactor.tick_step(vec![request_evt]);
    
    // Verify SideEffect Emitted
    let req_effect = effects_req.iter().find(|e| matches!(e, nexus::kernel::scheduler::SideEffect::RequestTranscription { .. }));
    assert!(req_effect.is_some(), "Should emit RequestTranscription when requested");
    
    if let Some(nexus::kernel::scheduler::SideEffect::RequestTranscription { segment_id }) = req_effect {
        assert_eq!(segment_id, &initial_seg_id);
    }
    
    // Verify State Update
    let seg_transcribing = reactor.state.audio_segments.get(&initial_seg_id).unwrap();
    assert_eq!(seg_transcribing.status, nexus::kernel::audio::segment::SegmentStatus::Transcribing, "Status should be Transcribing");

    // === Test 3: Provisional Text Ingestion ===
    println!("Step 3: Provisional Text");
    
    let text_evt = Event::Input(InputEvent {
        source: "ASR".to_string(),
        content: InputContent::ProvisionalText {
            content: "Hello World".to_string(),
            confidence: 0.9,
            source_id: initial_seg_id.clone(),
        }
    });
    
    let _ = reactor.tick_step(vec![text_evt]);
    
    let seg_done = reactor.state.audio_segments.get(&initial_seg_id).unwrap();
    assert_eq!(seg_done.status, nexus::kernel::audio::segment::SegmentStatus::Transcribed, "Status should be Transcribed");
    assert_eq!(seg_done.transcription.as_deref(), Some("Hello World"));

    println!("Phase E Tests Passed!");
}
