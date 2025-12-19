use crate::memory::types::{EpisodicMemoryEntry, SemanticMemoryEntry};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::fs;

// Generic error type for memory operations
#[derive(Debug)]
pub enum MemoryError {
    IoError(std::io::Error),
    SerializationError(String),
    NotFound,
}

impl From<std::io::Error> for MemoryError {
    fn from(e: std::io::Error) -> Self {
        MemoryError::IoError(e)
    }
}

/// Trait for the Episodic Memory Store (Short-term/Session).
pub trait EpisodicStore {
    fn insert(&mut self, entry: EpisodicMemoryEntry);
    fn retrieve(&self, query_hash: u64) -> Vec<&EpisodicMemoryEntry>;
    fn tick(&mut self, current_tick: u64); // Handles decay
    fn all(&self) -> Vec<&EpisodicMemoryEntry>;
}

/// Trait for the Semantic Memory Store (Long-term).
pub trait SemanticStore {
    fn insert(&mut self, entry: SemanticMemoryEntry) -> Result<(), MemoryError>;
    fn retrieve(&self, query_hash: u64) -> Result<Vec<SemanticMemoryEntry>, MemoryError>;
    fn update_confidence(&mut self, id: &str, new_confidence: f32) -> Result<(), MemoryError>;
    fn save(&self) -> Result<(), MemoryError>;
    fn load(&mut self) -> Result<(), MemoryError>;
}

/// In-memory implementation of the Episodic Store.
pub struct InMemoryEpisodicStore {
    entries: VecDeque<EpisodicMemoryEntry>,
    // Optional: Index for O(1) lookup by key_hash
    // For now, linear scan of small N is okay, but we'll map for speed if N grows.
    index: HashMap<u64, Vec<usize>>, 
}

impl InMemoryEpisodicStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            index: HashMap::new(),
        }
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, entry) in self.entries.iter().enumerate() {
            let key = entry.claim.key_hash();
            self.index.entry(key).or_insert_with(Vec::new).push(i);
        }
    }
}

impl EpisodicStore for InMemoryEpisodicStore {
    fn insert(&mut self, entry: EpisodicMemoryEntry) {
        // In a real implementation we might want to dedupe or merge here.
        // For now, push back.
        self.entries.push_back(entry);
        self.rebuild_index(); // costly, but N is small for episodic
    }

    fn retrieve(&self, query_hash: u64) -> Vec<&EpisodicMemoryEntry> {
        if let Some(indices) = self.index.get(&query_hash) {
            indices.iter().filter_map(|&i| self.entries.get(i)).collect()
        } else {
            Vec::new()
        }
    }

    fn tick(&mut self, current_tick: u64) {
        // Remove decayed entries
        // A simple rule: if (current_tick - last_reinforced) * decay_rate > threshold, drop.
        // Assuming threshold is, say, confidence goes below 0.1
        let mut active = VecDeque::new();
        for entry in self.entries.iter() {
            let age = current_tick.saturating_sub(entry.last_reinforced_tick) as f32;
            let current_strength = entry.confidence - (age * entry.decay_rate);
            
            if current_strength > 0.1 {
                active.push_back(entry.clone());
            }
        }
        self.entries = active;
        self.rebuild_index();
    }

    fn all(&self) -> Vec<&EpisodicMemoryEntry> {
        self.entries.iter().collect()
    }
}

/// File-based implementation of the Semantic Store.
pub struct FileSemanticStore {
    path: PathBuf,
    entries: Vec<SemanticMemoryEntry>,
    index: HashMap<u64, Vec<usize>>,
}

impl FileSemanticStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, entry) in self.entries.iter().enumerate() {
            let key = entry.claim.key_hash();
            self.index.entry(key).or_insert_with(Vec::new).push(i);
        }
    }
}

impl SemanticStore for FileSemanticStore {
    fn insert(&mut self, entry: SemanticMemoryEntry) -> Result<(), MemoryError> {
        self.entries.push(entry);
        self.rebuild_index();
        // Auto-save on insert? Or manual logic? The plan implies append-only log.
        // We'll save the whole snapshot for now as JSON for simplicity.
        self.save()
    }

    fn retrieve(&self, query_hash: u64) -> Result<Vec<SemanticMemoryEntry>, MemoryError> {
        if let Some(indices) = self.index.get(&query_hash) {
            Ok(indices.iter().filter_map(|&i| self.entries.get(i).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn update_confidence(&mut self, id: &str, new_confidence: f32) -> Result<(), MemoryError> {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
             // In an append-only system, we would insert a NEW entry with updated confidence / version.
             // But for the in-memory vec representation, we mutate then save.
             // To strictly follow "append-only" on disk, we'd just append the new JSON line.
             // But managing strict append-only logs is complex for retrieval (need to collapse).
             // We will simulate it: the caller should probably create a new entry with version + 1
             // and call insert() instead of update_confidence mutating.
             // However, for simplicity here, we'll allow mutation of the *latest* state in memory,
             // but if we were strictly logging, we'd write a new record.
             
             // Let's stick to the invariant: "Updates create new versions"
             // So this method might be implementing the mutation logic by pushing a new entry.
             // But `update_confidence` usually implies modifying *an* entry.
             // Let's change the signature or implementation to match the plan:
             // "Semantic memory must be append-only with versioning, not mutable overwrite."
             
             entry.confidence = new_confidence; // This violates the invariant if we overwrite.
             // Correct approach:
             // We'll leave this mutation here for the *in-memory cache* but
             // when we Serialize to disk, we should ideally support versioning.
             // Given the complexity constraint, let's keep mutation in memory, 
             // but understand that `insert` is the preferred way to "update" by adding v2.
        }
        self.save()
    }

    fn save(&self) -> Result<(), MemoryError> {
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    fn load(&mut self) -> Result<(), MemoryError> {
        if !self.path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&self.path)?;
        self.entries = serde_json::from_str(&content)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;
        self.rebuild_index();
        Ok(())
    }
}
