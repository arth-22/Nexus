use nexus::kernel::event::{Output, OutputId, OutputStatus, Event};
use nexus::kernel::time::Tick;
use nexus::kernel::crystallizer::{SymbolicSnapshot, Claim as SnapshotClaim};
use nexus::memory::{
    MemoryObserver, MemoryConsolidator, InMemoryEpisodicStore, FileSemanticStore,
    EpisodicStore, SemanticStore, Claim, EntityId, Predicate, ClaimValue, Modality
};
use std::path::PathBuf;
use std::fs;

// Helper to create a dummy output
fn create_output(tick: u64, content: &str, status: OutputStatus) -> Output {
    Output {
        id: OutputId { tick, ordinal: 0 },
        content: content.to_string(),
        status,
        proposed_at: Tick { frame: tick },
        committed_at: Some(Tick { frame: tick }),
        parent_id: None,
    }
}

// Helper to create a snapshot
fn create_snapshot(tick: u64, content: &str) -> SymbolicSnapshot {
    SymbolicSnapshot {
        claims: vec![SnapshotClaim {
            content: content.to_string(),
            confidence: 0.95,
            modality_support: vec!["Text".to_string()],
        }],
        base_uncertainty: 0.1,
        timestamp: Tick { frame: tick },
    }
}

#[tokio::test]
async fn test_no_memory_spam() {
    // TEST 1: Single fleeting thought -> Not remembered
    let mut observer = MemoryObserver::new();
    let mut consolidator = MemoryConsolidator::new();
    let mut episodic = InMemoryEpisodicStore::new();
    let temp_dir = std::env::temp_dir().join("nexus_test_spam");
    let _ = fs::remove_file(&temp_dir);
    let mut semantic = FileSemanticStore::new(temp_dir.clone());

    let output = create_output(100, "Fleeting thought", OutputStatus::SoftCommit);
    let snapshot = create_snapshot(100, "Fleeting thought");

    // Observe once
    observer.observe_crystallization(&output, &snapshot, 100);
    
    // Consolidate
    let candidates = observer.flush();
    assert_eq!(candidates.len(), 1, "Should have 1 candidate");
    
    consolidator.process(candidates, &mut episodic, &mut semantic, 100);

    // Verify NOT in episodic (needs repetition/time)
    let stored = episodic.all();
    assert!(stored.is_empty(), "Fleeting thought should not be promoted to episodic immediately");

    // Cleanup
    let _ = fs::remove_file(&temp_dir);
}

#[tokio::test]
async fn test_stability_promotion() {
    // TEST 2: Repeated, stable claims -> Remembered
    let mut observer = MemoryObserver::new();
    let mut consolidator = MemoryConsolidator::new();
    let mut episodic = InMemoryEpisodicStore::new();
    let temp_dir = std::env::temp_dir().join("nexus_test_stability");
    let _ = fs::remove_file(&temp_dir);
    let mut semantic = FileSemanticStore::new(temp_dir.clone());

    // Repeat the thought 5 times over 50 ticks
    for i in 0..5 {
        let tick = 100 + (i * 10);
        let output = create_output(tick, "Stable fact", OutputStatus::HardCommit);
        let snapshot = create_snapshot(tick, "Stable fact");
        
        observer.observe_crystallization(&output, &snapshot, tick);
        let candidates = observer.flush();
        consolidator.process(candidates, &mut episodic, &mut semantic, tick);
    }

    // Force promotion tick (last seen + 10)
    let final_tick = 200;
    consolidator.process(vec![], &mut episodic, &mut semantic, final_tick);

    // Verify in episodic
    let query_hash = {
        let stored = episodic.all();
        assert!(!stored.is_empty(), "Stable fact should be promoted to episodic");
        assert_eq!(stored[0].claim.object, ClaimValue::Text("Stable fact".to_string()));
        stored[0].claim.key_hash()
    };

    // Verify promotion to Semantic (Simulate sleep/long delay)
    // Run semantic promotion cycle (tick % 100 == 0)
    consolidator.process(vec![], &mut episodic, &mut semantic, 300);
    
    // Verify in semantic
    let semantic_hits = semantic.retrieve(query_hash).expect("Semantic DB error");
    
    // It might not be promoted if "modality" isn't explicitly Asserted in Observer mapping.
    // Observer maps output -> Modality::Asserted.
    // So it should work.
    
    assert!(!semantic_hits.is_empty(), "Stable hard commit should be promoted to semantic");

    // Cleanup
    let _ = fs::remove_file(&temp_dir);
}

#[tokio::test]
async fn test_revocation() {
    // TEST 3: Revision integrity
    // "My name is Sid" -> "Call me Alex"
    
    // Note: The current Consolidator implementation doesn't automatically HANDLE logical revocation 
    // in `process` loop via NLP. It just stores atoms.
    // But we want to verify that we CAN store contradictory claims and retrieve them,
    // and ideally the *latest* or *highest confidence* wins in retrieval.
    
    let mut episodic = InMemoryEpisodicStore::new();
    let temp_dir = std::env::temp_dir().join("nexus_test_revocation");
    let _ = fs::remove_file(&temp_dir);
    let semantic = FileSemanticStore::new(temp_dir.clone()); // Unused logic for simple retrieval test

    // Store "Name is Sid"
    let claim1 = Claim::new(
        EntityId::User,
        Predicate::Is,
        ClaimValue::Text("Sid".to_string()),
        Modality::Asserted
    );
    
    use nexus::memory::EpisodicMemoryEntry;
    episodic.insert(EpisodicMemoryEntry {
        claim: claim1.clone(),
        confidence: 0.9,
        created_at_tick: 100,
        last_reinforced_tick: 100,
        decay_rate: 0.01,
    });

    // Store "Name is Alex" (Later, same subject/predicate)
    let claim2 = Claim::new(
        EntityId::User,
        Predicate::Is,
        ClaimValue::Text("Alex".to_string()),
        Modality::Asserted
    );
     episodic.insert(EpisodicMemoryEntry {
        claim: claim2.clone(),
        confidence: 0.95, // Higher confidence (recency bias simulation)
        created_at_tick: 200,
        last_reinforced_tick: 200,
        decay_rate: 0.01,
    });
    
    // Retrieve
    use nexus::memory::MemoryRetriever;
    // Hash based on Subject + Predicate
    let query_hash = claim1.key_hash(); 
    
    let results = MemoryRetriever::retrieve(query_hash, &episodic, &semantic);
    
    assert_eq!(results.len(), 2, "Should return both conflicting memories");
    
    // Check ordering (Confidence High -> Low)
    assert_eq!(results[0].content.object, ClaimValue::Text("Alex".to_string()));
    assert_eq!(results[1].content.object, ClaimValue::Text("Sid".to_string()));
    
    // Cleanup
    let _ = fs::remove_file(&temp_dir);
}
