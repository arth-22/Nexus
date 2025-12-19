# Nexus Cortex (Internal) 🧠

> **WARNING**: This is the core kernel of the Nexus project. Strict architectural invariants apply. Do not modify `kernel/` without deep understanding of the Reactor loop.

Nexus is an event-driven AI kernel designed for high-latency, long-horizon agentic workflows. It implements a "Stream of Thought" architecture where state, time, and causality are rigorously modeled.

## 🏗 System Architecture

The system is built around a **Reactor Pattern** (`src/kernel/reactor.rs`).

### The Core Loop
1.  **Tick**: The system heartbeat advances. Time is discrete.
2.  **Input Processing**: External events (Audio, Vision, User Input) are reduced into `SharedState`.
3.  **Sidecar Processing**: 
    *   **Monitor**: Checks for anomalies (interruptions, confidence drops).
    *   **LHIM** (Intent Manager): Applies decay to long-horizon goals.
    *   **Memory**: Observes latents and consolidates episodic memories.
4.  **Planning**: If the state is quiescent, the `AsyncPlanner` is dispatched to the LLM.
5.  **Crystallization**: Detailed thought processes are "crystallized" into text output only when the `Crystallizer` gate permits (based on stability and confidence).

### Directory Structure

*   **`src/kernel/`**: The unyielding core.
    *   `state.rs`: Defines `SharedState` and `StateDelta`. **LAW**: State mutates *only* via `reduce(delta)`.
    *   `reactor.rs`: The event loop driver.
    *   `crystallizer.rs`: The output gatekeeper.
*   **`src/memory/`**: Dual-process memory (Episodic + Semantic).
    *   *Note*: Semantic memory is currently file-backed (`nexus_semantic_memory.json`) for dev iteration.
*   **`src/intent/`**: Long-Horizon Intent Manager.
    *   Enforces "Interruption Supremacy" (responsiveness > persistence).
*   **`src/monitor/`**: Self-Correction ("Super-Ego").
    *   Adjusts `MetaLatents` (e.g., `confidence_penalty`) to bias the planner.
*   **`src/planner/`**: LLM Integration.
    *   Currently uses a Stub or Async HTTP client.

## ⚠️ Developer Invariants ("The Laws")

1.  **Strict Causality**: You CANNOT mutate `SharedState` directly. You must emit a `StateDelta`.
2.  **Non-Blocking Reactor**: The `tick_step` function must never await I/O. All I/O is handled by the `Scheduler` (SideEffects) or `AsyncPlanner`.
3.  **Passive Observation**: The Planner does not "control" the body directly; it emits `Intent`s which are scheduled. The Cortex can override or ignore them based on `MetaLatents`.
4.  **No Agentic Drift**: The Intent Manager (`lhim`) provides *context* to the planner but never triggers actions itself.

## 🛠 Development Workflow

### Setup
Ensure you have the latest stable Rust toolchain and Audio dependencies:
```bash
# Mac (CoreAudio is built-in)
# Linux
sudo apt install libasound2-dev
```

### Running the Kernel

The system requires two components running in parallel: the **Brain (LLM)** and the **Body (Cortex)**.

#### 1. Start the Brain
Nexus expects an LLM server compatible with the llama.cpp API running on port 8080.
**Prerequisite**: You must have `llama-server` installed (part of [llama.cpp](https://github.com/ggerganov/llama.cpp)).

```bash
# Detected via 'which llama-server' and local file check:
/opt/homebrew/bin/llama-server -m Llama-3.2-1B-Instruct-Q4_K_M.gguf -c 2048 --port 8080

# SAFE MODE (If the above crashes with "ggml-metal"):
# Run on CPU only (slower but stable)
/opt/homebrew/bin/llama-server -m Llama-3.2-1B-Instruct-Q4_K_M.gguf -c 2048 --port 8080 -ngl 0

# NOTE: CPU mode is slow. You MUST increase the planner timeout when running the kernel:
# NEXUS_PLANNER_TIMEOUT_MS=3000 cargo run --bin live_nexus
```
*   **Server**: Installed via Homebrew (`/opt/homebrew/bin/llama-server`).
*   **Model**: Located in current directory (`Llama-3.2-1B-Instruct-Q4_K_M.gguf`).
*   **Context**: 2048 context window is sufficient for current prompts.

#### 2. Start the Body (Cortex)
In a separate terminal, run the reactor loop:

```bash
# Run with default logging (INFO)
cargo run --bin live_nexus

# Run with verbose debugging
RUST_LOG=debug cargo run --bin live_nexus
```
*   **Stdin**: Type messages to simulate user input.
*   **Logs**: You should see "Reactor Pipeline Started" and "Audio Capture Initialized".

### Verification
We use strict behavior-driven tests for critical paths. **Run these before pushing.**

```bash
# Full Suite
cargo test

# Intent System (Part IX)
cargo test --test phase9_intent_tests

# Memory System (Part VII)
cargo test --test phase7_memory_tests
```

### Common Tasks
*   **Adding a new System Capability**: Implement it as a Sidecar in `src/kernel/reactor.rs` that emits `StateDelta`s.
*   **Tuning Behavior**: Adjust constants in `src/monitor/monitor.rs` (Decay rates, penalties).
*   **Modifying Planner Prompting**: Check `src/planner/` (though prompt logic involves the `StateSnapshot`).
