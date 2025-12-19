use nexus::kernel::reactor::Reactor;
use nexus::kernel::event::{Event, InputEvent, InputContent, AudioSignal};
use nexus::kernel::intent::types::{IntentState, IntentStability, IntentHypothesis};
use nexus::kernel::scheduler::SideEffect;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_no_reflex_on_stable() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());

    // 1. Inject Clear Command ("Turn off the lights")
    let inputs = vec![
        Event::Input(InputEvent {
            source: "Test".to_string(),
            content: InputContent::ProvisionalText {
                content: "Turn off the lights".to_string(),
                confidence: 0.95,
                source_id: "seg_1".to_string(),
            }
        })
    ];

    let effects = reactor.tick_step(inputs);

    // 2. Verify State is Stable
    match &reactor.state.intent_state {
        IntentState::Stable(cand) => {
            assert_eq!(cand.hypothesis, IntentHypothesis::Command);
            assert_eq!(cand.stability, IntentStability::Stable);
        },
        _ => panic!("Expected Stable intent, got {:?}", reactor.state.intent_state),
    }

    // 3. Verify NO Audio Output (DialogueAct::Wait)
    let has_audio = effects.iter().any(|e| matches!(e, SideEffect::SpawnAudio(..)));
    assert!(!has_audio, "Reactor should NOT speak on Stable intent (Reflex check)");
}

#[tokio::test]
async fn test_clarification_on_ambiguity() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());

    // 1. Inject Ambiguous Inquiry ("What if I...")
    let _inputs = vec![
        Event::Input(InputEvent {
            source: "Test".to_string(),
            content: InputContent::ProvisionalText {
                content: "What if I...".to_string(),
                confidence: 0.6, // Low enough to trigger clarification if unstable?
                // Heuristic in Arbitrator for "?" is high confidence inquiry if length > 10.
                // "What if I..." is > 10? No, 12 chars.
                // Let's rely on heuristic: "what" + "maybe" or length.
                // My arbitrator logic: contains "what", len > 10 => Stable Inquiry (0.85).
                // Wait, "What if I..." might be Stable Inquiry by my simple regex.
                // Let's try "maybe what is this" to trigger Unstable.
                source_id: "seg_2".to_string(),
            }
        })
    ];
    
    // Using a phrase that triggers Unstable/Clarification logic
    // Logic: contains "?" AND (contains "maybe" OR len < 10) -> Unstable Inquiry
    let inputs_ambiguous = vec![
        Event::Input(InputEvent {
            source: "Test".to_string(),
            content: InputContent::ProvisionalText {
                content: "maybe what?".to_string(),
                confidence: 0.6,
                source_id: "seg_2".to_string(),
            }
        })
    ];

    let effects = reactor.tick_step(inputs_ambiguous);

    // 2. Verify State is Forming (Unstable)
    match &reactor.state.intent_state {
        IntentState::Forming(cands) => {
            assert!(!cands.is_empty());
             // Ensure best candidate is Inquiry/Unstable
            let best = cands.iter().max_by(|a,b| a.confidence.partial_cmp(&b.confidence).unwrap()).unwrap();
            assert_eq!(best.hypothesis, IntentHypothesis::Inquiry);
            assert_eq!(best.stability, IntentStability::Unstable);
        },
        _ => panic!("Expected Forming intent, got {:?}", reactor.state.intent_state),
    }

    // 3. Verify Audio Output (DialogueAct::AskClarification)
    let audio_effect = effects.iter().find(|e| matches!(e, SideEffect::SpawnAudio(..)));
    assert!(audio_effect.is_some(), "Reactor SHOULD speak to clarify");
    
    if let Some(SideEffect::SpawnAudio(_, msg)) = audio_effect {
        assert_eq!(msg, "Do you want me to respond?"); // Strict checking of non-leading question
    }
}

#[tokio::test]
async fn test_interruption_suspends_intent() {
    let (tx, rx) = mpsc::channel(100);
    let mut reactor = Reactor::new(rx, tx.clone());

    // 1. Establish State (Forming)
    reactor.state.intent_state = IntentState::Forming(vec![
        nexus::kernel::intent::types::IntentCandidate {
            id: "test".to_string(),
            hypothesis: IntentHypothesis::Inquiry,
            confidence: 0.8,
            source_symbol_ids: vec!["s1".to_string()],
            stability: IntentStability::Unstable,
        }
    ]);

    // 2. Inject SpeechStart (Interruption)
    let inputs = vec![
        Event::Input(InputEvent {
            source: "VAD".to_string(),
            content: InputContent::Audio(AudioSignal::SpeechStart)
        })
    ];

    let _ = reactor.tick_step(inputs);

    // 3. Verify State is Suspended
    match &reactor.state.intent_state {
        IntentState::Suspended(cand) => {
            assert_eq!(cand.id, "test");
        },
        _ => panic!("Expected Suspended intent after interruption, got {:?}", reactor.state.intent_state),
    }
}
