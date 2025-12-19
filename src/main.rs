use nexus::kernel::reactor::Reactor;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging/tracing
    tracing_subscriber::fmt::init();
    tracing::info!("Nexus Kernel Booting...");

    // Create event channel
    let (tx, rx) = mpsc::channel(100);

    // 2. Setup Reactor
    let config = nexus::kernel::reactor::ReactorConfig { safe_mode: false };
    let mut reactor = Reactor::new(rx, tx.clone(), config);

    // Spawn Reactor
    let reactor_handle = tokio::spawn(async move {
        reactor.run().await;
    });

    tracing::info!("Nexus Kernel Active. Press Ctrl+C to stop.");
    
    // In a real system, we'd spawn input listeners here ensuring they have 'tx' cloned.
    // For Phase 0 skeleton, we just wait.
    reactor_handle.await?;

    Ok(())
}
