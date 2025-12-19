# Nexus Desktop Shell

The detailed UI for Nexus on macOS.

## Prerequisites

1. **Rust**: Ensure Rust is installed (`rustUp`).
2. **Tauri CLI**: You need the Tauri CLI to run the application.
   ```bash
   cargo install tauri-cli
   ```

## How to Run

1. Navigate to the Tauri backend directory:
   ```bash
   cd shell/src-tauri
   ```

2. Run the development application:
   ```bash
   cargo tauri dev
   ```

This will compile the Rust backend, bundle the `shell` frontend, and launch the floating Nexus window.
