pub mod kernel;
pub mod planner;
pub mod audio;
pub mod outputs;
pub mod vision;
pub mod memory;
pub mod monitor;
// pub mod intent; // Legacy - Removed in Phase I

// Re-export specific items if needed for convenient access
pub use kernel::reactor::Reactor;
