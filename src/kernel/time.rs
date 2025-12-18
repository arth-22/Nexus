#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick {
    pub frame: u64,
}

pub const TICK_MS: u64 = 20;

impl Tick {
    pub fn new() -> Self {
        Tick { frame: 0 }
    }

    pub fn next(&self) -> Self {
        Tick { frame: self.frame + 1 }
    }
}
