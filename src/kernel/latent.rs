use crate::kernel::time::Tick;

#[derive(Debug, Clone, PartialEq)]
pub enum Modality {
    Audio,
    Visual,
    Text,
}

#[derive(Debug, Clone)]
pub struct LatentSlot {
    pub values: Vec<f32>,
    pub confidence: f32, // 0.0 - 1.0
    pub created_at: Tick,
    pub modality: Modality,
    pub decay_rate: f32, // Lambda for exp decay
}

#[derive(Debug, Clone, Default)]
pub struct LatentState {
    pub slots: Vec<LatentSlot>,
}

impl LatentState {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn global_uncertainty(&self) -> f32 {
        if self.slots.is_empty() {
             // Or 1.0 (Max uncertainty)?
             // If "Empty" means "I know nothing", maybe 1.0?
             // But if "Uncertainty" implies "Confusion", then Empty = Clear?
             // Let's assume uncertainty is Entropy.
             // Empty slate = Low Entropy (Predictable).
             // Conflicting latents = High Entropy.
             // For now, return sum of confidences inverted?
             // Plan says "derived function".
             // Let's use average confidence of active slots?
             // Actually, more slots might mean MORE context.
             // Let's stick to a simple placeholder: 1.0 - avg(confidence).
             return 0.0;
        }
        
        let avg_conf: f32 = self.slots.iter().map(|s| s.confidence).sum::<f32>() / self.slots.len() as f32;
        (1.0 - avg_conf).max(0.0)
    }
}
