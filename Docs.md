# Nexus Cortex Documentation

> **Nexus** is an event-driven AI kernel for orchestrating high-latency, long-horizon agentic workflows without sacrificing real-time responsiveness.

Built in **Rust** using `tokio` for async runtime, `cpal` for audio capture, and `img_hash`/`xcap` for vision processing.

---

## Table of Contents
1. [Introduction](#1-introduction)
2. [Core Architecture](#2-core-architecture)
3. [Event & Data Types](#3-event--data-types)
4. [Latent State System](#4-latent-state-system)
5. [Key Modules](#5-key-modules)
6. [Input Pipelines](#6-input-pipelines)
7. [Memory System](#7-memory-system)
8. [Developer Guide](#8-developer-guide)
9. [Testing](#9-testing)
10. [API Quick Reference](#10-api-quick-reference)
11. [Directory Structure](#11-directory-structure)

---

## 1. Introduction

Unlike traditional request-response chatbots, Nexus implements a **"Stream of Thought"** architecture that models time, causality, and internal state rigorously. The system enables an agent to:

- **Think** (Planner) — Dispatch high-latency LLM calls asynchronously
- **Feel** (Monitor) — Self-observe and adjust metacognitive latents
- **Act** (Reactor/Crystallizer) — Commit outputs only when stable

### Design Philosophy
- **Non-blocking by design**: All I/O is asynchronous; the core loop is pure
- **Strict causality**: State mutates ONLY via `StateDelta` reduction
- **Interruption supremacy**: Responsiveness always trumps persistence

---

## 2. Core Architecture

### 2.1 The Reactor Pattern

The **Reactor** (`src/kernel/reactor.rs`) is a non-blocking event loop:

```
┌─────────────────────────────────────────────────────────┐
│                    REACTOR LOOP                         │
├─────────────────────────────────────────────────────────┤
│  1. Tick Advance      →  Logical clock increments       │
│  2. Event Ingestion   →  Audio/Vision/Text reduced      │
│  3. Sidecar Processing:                                 │
│     • Monitor         →  Adjust MetaLatents             │
│     • LHIM            →  Decay long-horizon intents     │
│     • Memory          →  Observe & consolidate          │
│  4. Planning          →  Dispatch to AsyncPlanner       │
│  5. Crystallization   →  Gate output commitment         │
└─────────────────────────────────────────────────────────┘
```

- **Tick-Based Time**: Time is discrete, quantized into `Tick` structs
- **Pure `tick_step()`**: Calculates next state from current state + events; **never awaits**
- **Causal Chain**: `Events → State Reduction → Side Effects`

### 2.2 Shared State & Deltas

State is encapsulated in `SharedState` (`src/kernel/state.rs`).

**The Law of Mutation**: You cannot mutate `SharedState` directly. You must emit a `StateDelta`:

```rust
pub enum StateDelta {
    InputReceived(InputEvent),
    OutputProposed(Output),
    OutputCommitted(OutputId),
    OutputCanceled(OutputId),
    TaskCanceled(String),
    VisualStateUpdate { hash: u64, stability: f32 },
    LatentUpdate { slot: LatentSlot },
    MetaLatentUpdate { delta: MetaLatents },
    IntentUpdate { intent: LongHorizonIntent },
    Tick(Tick),
}
```

The `reduce()` method applies deltas atomically:
```rust
state.reduce(StateDelta::InputReceived(event));
```

---

## 3. Event & Data Types

### 3.1 Event Enum
`Event` (`src/kernel/event.rs`) represents all signals flowing through the system:

| Variant | Purpose |
|---------|---------|
| `Event::Input(InputEvent)` | External signals from sensors |
| `Event::PlanProposed(PlanningEpoch, Intent)` | LLM-generated intent |

### 3.2 InputEvent & Content
```rust
pub struct InputEvent {
    pub source: String,        // "Audio", "Vision", "User"
    pub content: InputContent,
}

pub enum InputContent {
    Text(String),
    Audio(AudioSignal),        // SpeechStart, SpeechEnd
    Visual(VisualSignal),      // PerceptUpdate { hash, distance }
}
```

### 3.3 Output Lifecycle
Outputs follow a strict lifecycle managed by the `Crystallizer`:

```
Draft → SoftCommit → HardCommit
  │         │
  └────────────→ Canceled
```

| Status | Meaning |
|--------|---------|
| `Draft` | Proposed, not yet visible |
| `SoftCommit` | Visible but retractable |
| `HardCommit` | Durable, cannot be revoked |
| `Canceled` | Aborted before/during commit |

---

## 4. Latent State System

The **Latent State** (`src/kernel/latent.rs`) represents uncertain, decaying knowledge:

### 4.1 LatentSlot
```rust
pub struct LatentSlot {
    pub values: Vec<f32>,      // Embedding or feature vector
    pub confidence: f32,       // 0.0 - 1.0
    pub created_at: Tick,
    pub modality: Modality,    // Audio, Visual, Text
    pub decay_rate: f32,       // Lambda for exponential decay
}
```

### 4.2 Modality
```rust
pub enum Modality {
    Audio,   // From microphone
    Visual,  // From screen capture
    Text,    // From LLM or user input
}
```

### 4.3 Uncertainty Calculation
`LatentState::global_uncertainty()` computes overall system confidence:
```rust
// Higher average confidence → Lower uncertainty
uncertainty = 1.0 - average(slot.confidence for all slots)
```

---

## 5. Key Modules

### 5.1 Planner (The Brain)
**Location**: `src/planner/`

| File | Purpose |
|------|---------|
| `async_planner.rs` | HTTP client for LLM with abort capability |
| `types.rs` | `Intent`, `StateSnapshot`, `PlanningEpoch` |

The `AsyncPlanner`:
1. Takes a `StateSnapshot` (sanitized state view)
2. Sends to LLM server (llama.cpp compatible)
3. Returns an `Intent` enum

```rust
pub enum Intent {
    BeginResponse { confidence: f32 },
    Delay { ticks: u64 },
    AskClarification { context: String },
    ReviseStatement { ref_id: OutputId, correction: String },
    DoNothing,
}
```

**Key Behavior**: If new inputs arrive while planning, the current plan is **aborted**.

### 5.2 Monitor (The Super-Ego)
**Location**: `src/monitor/`

The `SelfObservationMonitor` observes events and adjusts `MetaLatents`:

| Observation | Effect |
|-------------|--------|
| `UnexpectedInterruption` | ↑ `interruption_sensitivity` |
| `UserCorrection` | ↑ `confidence_penalty` |
| `ResponseTruncation` | ↑ `interruption_sensitivity` |
| `StableAlignment` | ↓ Both (healing) |

**Invariant**: MetaLatents decay towards 0.0 over time (recovery).

### 5.3 Crystallizer (The Gatekeeper)
**Location**: `src/kernel/crystallizer.rs`

Decides **when** to speak via `check_gate()`:

```rust
pub fn check_gate(state: &SharedState) -> CrystallizationDecision {
    // Returns: Deny, Delay { ms }, AllowPartial, AllowHard
}
```

| Decision | Meaning |
|----------|---------|
| `Deny` | Too unstable; suppress output |
| `Delay { ms }` | Wait for stability |
| `AllowPartial` | Soft-commit (retractable) |
| `AllowHard` | Hard-commit (durable) |

**Pure Function**: `extract_snapshot()` deterministically extracts `Claim`s from state.

### 5.4 Scheduler
**Location**: `src/kernel/scheduler.rs`

Converts `Intent` → `(StateDelta, SideEffect)`:

```rust
pub enum SideEffect {
    Log(String),
    SpawnAudio(OutputId, String),
}
```

### 5.5 Long-Horizon Intent Manager (LHIM)
**Location**: `src/intent/`

Maintains intent consistency over time:

| Status | Meaning |
|--------|---------|
| `Active` | Currently being pursued |
| `Suspended` | Paused due to interruption |
| `Dissolved` | Confidence decayed below threshold |

**Key Principle**: "Interruption Supremacy" — responsiveness always trumps persistence.

---

## 6. Input Pipelines

### 6.1 Audio Pipeline
**Location**: `src/audio/`

| File | Purpose |
|------|---------|
| `capture.rs` | Real-time mic input via `cpal` |
| `processing.rs` | VAD (Voice Activity Detection) via `webrtc-vad` |

**Supported Sample Rates**: 8kHz, 16kHz, 32kHz, 48kHz (VAD requirement)

**Flow**:
```
Microphone → RingBuffer → VAD → AudioSignal Events (SpeechStart/End)
```

### 6.2 Vision Pipeline
**Location**: `src/vision/pipeline.rs`

Captures screen at ~5 FPS and computes perceptual hashes:

```
Screen Capture → Resize → Gradient Hash (8x8) → Hamming Distance → VisualSignal
```

**Key Behavior**: Capture failure degrades to silence (no event), not error.

---

## 7. Memory System

### 7.1 Architecture Overview
**Location**: `src/memory/`

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Observer  │───▶│ Consolidator│───▶│   Stores    │
│  (Sensor)   │    │  (Cortex)   │    │ Epi + Sem   │
└─────────────┘    └─────────────┘    └─────────────┘
```

### 7.2 Dual-Store Model

| Store | Trait | Implementation | Purpose |
|-------|-------|----------------|---------|
| Episodic | `EpisodicStore` | `InMemoryEpisodicStore` | Session-scale, high-fidelity |
| Semantic | `SemanticStore` | `FileSemanticStore` | Long-term, compressed |

### 7.3 Memory Types

**Claim** (atomic unit):
```rust
pub struct Claim {
    pub subject: EntityId,    // System, User, Topic(String)
    pub predicate: Predicate, // Prefers, Is, Knows, Capability, Context
    pub object: ClaimValue,   // Text(String), Boolean, Number
    pub modality: Modality,   // Asserted, Inferred, Observed
}
```

**MemoryCandidate** → **EpisodicMemoryEntry** → **SemanticMemoryEntry**

### 7.4 Consolidation Rules

| Promotion | Condition |
|-----------|-----------|
| Working → Episodic | Persisted >5 ticks OR intensity >3.0 |
| Episodic → Semantic | Confidence >0.9 AND Modality = Asserted |

---

## 8. Developer Guide

### 8.1 Invariants ("The Laws")

1. **Strict Causality**: Mutate state ONLY via `StateDelta`
2. **Non-Blocking Reactor**: Never `await` inside `tick_step`
3. **Passive Observation**: Planner suggests; Kernel decides
4. **No Agentic Drift**: LHIM provides context only, never triggers actions

### 8.2 Setup & Running

**Prerequisites**: Rust toolchain, `llama-server`

**Step 1: Start LLM Server**
```bash
# GPU Mode (faster)
/opt/homebrew/bin/llama-server -m Llama-3.2-1B-Instruct-Q4_K_M.gguf -c 2048 --port 8080

# CPU Mode (stable, slower)
/opt/homebrew/bin/llama-server -m Llama-3.2-1B-Instruct-Q4_K_M.gguf -c 2048 --port 8080 -ngl 0
```

**Step 2: Run Kernel**
```bash
# Default (200ms timeout)
cargo run --bin live_nexus

# Extended timeout for CPU mode
NEXUS_PLANNER_TIMEOUT_MS=3000 cargo run --bin live_nexus

# Verbose debugging
RUST_LOG=debug cargo run --bin live_nexus
```

### 8.3 Common Tasks

| Task | How |
|------|-----|
| Add system capability | Implement as Sidecar in `reactor.rs` emitting `StateDelta` |
| Tune behavior | Adjust constants in `src/monitor/monitor.rs` |
| Modify prompting | Edit `src/planner/async_planner.rs` |

---

## 9. Testing

### 9.1 Test Suite Overview

| Test File | Phase | Focus |
|-----------|-------|-------|
| `phase1_tests.rs` | I | Core reactor & state reduction |
| `phase2_audio_tests.rs` | II | Audio capture & VAD |
| `phase3_vision_tests.rs` | III | Vision pipeline & hashing |
| `phase5_latent_tests.rs` | V | Latent state & uncertainty |
| `phase6_crystallization_tests.rs` | VI | Output gating & lifecycle |
| `phase7_memory_tests.rs` | VII | Memory consolidation |
| `phase8_monitor_tests.rs` | VIII | Self-observation & healing |
| `phase9_intent_tests.rs` | IX | LHIM & interruption handling |
| `verification_test.rs` | — | Integration verification |

### 9.2 Running Tests
```bash
# Full suite
cargo test

# Specific phase
cargo test --test phase9_intent_tests
cargo test --test phase7_memory_tests

# With output
cargo test -- --nocapture
```

---

## 10. API Quick Reference

### Core Structs

| Struct | Location | Purpose |
|--------|----------|---------|
| `Reactor` | `kernel/reactor.rs` | Main event loop driver |
| `SharedState` | `kernel/state.rs` | Central state container |
| `AsyncPlanner` | `planner/async_planner.rs` | LLM interface |
| `SelfObservationMonitor` | `monitor/monitor.rs` | Metacognition |
| `LongHorizonIntentManager` | `intent/manager.rs` | Goal persistence |
| `MemoryConsolidator` | `memory/consolidator.rs` | Memory promotion |
| `MemoryObserver` | `memory/observer.rs` | Memory sensing |

### Key Functions

| Function | Location | Purpose |
|----------|----------|---------|
| `tick_step()` | `Reactor` | Pure state transition |
| `reduce()` | `SharedState` | Apply `StateDelta` |
| `check_gate()` | `crystallizer.rs` | Output decision |
| `dispatch()` | `AsyncPlanner` | Send to LLM |
| `abort()` | `AsyncPlanner` | Cancel in-flight plan |

---

## 11. Directory Structure

```
src/
├── kernel/                    # Core event loop & state
│   ├── reactor.rs             # Main loop driver
│   ├── state.rs               # SharedState & StateDelta
│   ├── crystallizer.rs        # Output gating
│   ├── scheduler.rs           # Intent → SideEffect
│   ├── event.rs               # Event types
│   ├── latent.rs              # LatentSlot & uncertainty
│   ├── time.rs                # Tick definitions
│   └── cancel.rs              # Task cancellation
├── planner/                   # LLM integration
│   ├── async_planner.rs       # HTTP client with abort
│   ├── types.rs               # Intent, StateSnapshot
│   └── stub.rs                # Mock planner for testing
├── monitor/                   # Self-correction
│   ├── monitor.rs             # SelfObservationMonitor
│   └── types.rs               # SelfObservation enum
├── memory/                    # Dual-process memory
│   ├── store.rs               # EpisodicStore, SemanticStore traits
│   ├── consolidator.rs        # Promotion logic
│   ├── observer.rs            # Memory sensing
│   ├── retriever.rs           # Query interface
│   └── types.rs               # Claim, MemoryCandidate
├── intent/                    # Long-horizon goals
│   ├── manager.rs             # LongHorizonIntentManager
│   └── types.rs               # LongHorizonIntent, IntentStatus
├── audio/                     # Audio input
│   ├── capture.rs             # cpal microphone capture
│   └── processing.rs          # VAD processing
├── vision/                    # Vision input
│   └── pipeline.rs            # Screen capture & hashing
├── outputs/                   # Output realization
│   ├── realizer.rs            # Text output formatting
│   ├── text.rs                # Text utilities
│   └── mock_audio.rs          # Audio output stub
├── lib.rs                     # Public module exports
├── main.rs                    # Entry point
└── bin/
    └── live_nexus.rs          # Live system binary

tests/
├── phase1_tests.rs            # Core reactor
├── phase2_audio_tests.rs      # Audio pipeline
├── phase3_vision_tests.rs     # Vision pipeline
├── phase5_latent_tests.rs     # Latent state
├── phase6_crystallization_tests.rs  # Output gating
├── phase7_memory_tests.rs     # Memory system
├── phase8_monitor_tests.rs    # Monitor & healing
├── phase9_intent_tests.rs     # Intent management
└── verification_test.rs       # Integration
```

---

*Documentation generated from codebase analysis. For questions, consult the source files directly.*
