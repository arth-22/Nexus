# Nexus Presence Specification (Phase A)

> [!IMPORTANT]
> **STATUS: FROZEN**
> **ARCHITECTURAL LOCKPOINT**
> This document defines the non-negotiable behavioral laws of Nexus on a laptop.
> Any modification to this file is a **BREAKING ARCHITECTURAL CHANGE** and requires explicit justification.
> These rules override all future feature requests, UI conveniences, and engagement metrics.

## 1. The Presence Contract
Nexus on a laptop must satisfy all of the following at all times. If a design violates one of these, it is a failure.

*   **Continuity**: Nexus persists across hours. Closing the UI does not reset cognition. There is no exposed "session" concept. Persistence is local-first; cloud sync is not implied.
*   **Non-demanding**: Nexus **never** requires input, **never** signals urgency by default, and **never** forces turn-taking.
*   **Interruptible**: Users can speak or type at any time. Nexus stops immediately when interrupted. No apologies, no friction.
*   **Silence-safe**: Silence is a valid, stable state. No idle animations. No "waiting" indicators.
*   **Peripheral**: Nexus does not compete with the user's primary task. It is available without being central.

## 2. Authority Hierarchy (Conflict Resolution)
When requirements conflict, these precedence rules apply strictly:

1.  **User Interruption** > Nexus Output
2.  **Silence** > Speculative Output
3.  **Background Continuity** > UI Lifecycle
4.  **Presence Contract** > Feature Convenience

*The user's flow always wins. Silence is always preferred over noise. The background process is the truth; the UI is just a view.*

## 3. Engagement Anti-Definition
To prevent "engagement" from becoming a metric for noise, we explicitly define what Engagement is **NOT**:

*   **Engagement ≠ Responsiveness**: Just because Nexus is "engaged" does not mean it must reply. It can be engaged while listening in silence.
*   **Engagement ≠ Output**: Generating text or audio is not the only form of engagement. Thinking, observing, and holding context are valid engaged states.
*   **Engagement ≠ Visible Activity**: Screen activity is not a proxy for utility. Nexus can be highly useful while invisible.

## 4. The Laptop "Being There" Model
Nexus is defined as a **background resident process** with an **optional foreground visibility** and **continuous internal cognition**.

*   **Correct Model**: Window manager daemon, media engine, language server.
*   **Incorrect Model**: Chat app, document editor, task manager.

**Implementation Implication**: You must separate the **Nexus Core Process** (always running) from the **UI Surface** (can appear/disappear).

## 5. Lifecycle States
These states exist in the core process, independent of UI.

1.  **Dormant**: Process running, no listening, no visible UI, memory intact.
2.  **Attentive**: Listening enabled, no output, monitoring interruptions, waiting for stability.
3.  **Engaged**: Actively processing input, possibly drafting output, fully interruptible.
4.  **Quietly Holding**: Long-horizon intent exists, no immediate action, waiting for reinforcement or return. Quietly Holding represents sustained intent across time, not immediate readiness to act.
5.  **Suspended**: User explicitly paused Nexus. Cognition frozen.

## 6. Prohibited Behaviors
Nexus is **explicitly forbidden** from the following actions on a laptop:

*   Pop notifications unprompted.
*   Steal window focus.
*   Overlay content on other apps.
*   Speak without being spoken to.
*   Auto-resume conversations visually.
*   Summarize unless asked or gated.
*   Display "thinking" indicators (e.g., bouncing dots) during silence.

## 7. Non-Goals
Nexus explicitly **DOES NOT** optimize for:

*   **Speed**: Correctness and calmness are more important than raw latency.
*   **Verbosity**: Short, dense, or silent responses are preferred over lengthy explanations.
*   **Engagement Metrics**: We do not care how often the user interacts with Nexus, only that it is useful when they do.
*   **Constant Availability**: If the system needs to sleep or recover, it should do so rather than degrading.

## 8. Input Philosophy
Input is **ambient**, not transactional.

*   Input does not have a "send" moment.
*   Input can be partial.
*   Input can be abandoned.
*   Input can overlap with output.
*   Input does not imply response.

## 9. Output Philosophy
Output is **optional**, **speculative**, **retractable**, and **subordinate** to user flow.

*   Output must never appear just to show activity.
*   Output must never fill silence.
*   Output must never prove intelligence.
*   Output must never justify existence.

## 10. The "Edge of Awareness" Rule
Nexus lives at the edge of user awareness, not the center.

*   UI should not be full-screen by default.
*   No persistent flashing indicators.
*   No unread counters.
*   No "new message" badges.

## 11. Trust & Permission Boundaries
Permissions must be **explicit**, **revocable**, **visible**, and **local-first**.

*   User must always know when audio/vision is active.
*   User must always know what is being remembered.
*   No dark patterns.
*   No hidden background behavior.
