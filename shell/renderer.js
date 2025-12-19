// Phase C: Renderer Logic
// Invariants:
// 1. UI is a mirror (Visualizes Core state).
// 2. UI has no memory (No local persistence).
// 3. UI tolerates silence (No waiting animations).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const dom = {
    body: document.body,
    canvas: document.getElementById('canvas'),
    input: document.getElementById('ambient-input'),
    mic: document.getElementById('mic-toggle'),
    indicator: document.getElementById('presence-label'),
    // Cache onboarding elements directly
    onboardingOverlay: document.getElementById('onboarding-overlay'),
};
// Helper vars to avoid 'dom' scoping issues if any
const onboardingText = document.getElementById('onboarding-text');
const onboardingButton = document.getElementById('onboarding-continue');

// --- Phase K: Onboarding Content ---
const ONBOARDING_SCREENS = [
    `Nexus is a system that listens to unfinished thought.

You don't need to address it directly.
You don't need to finish sentences.

Sometimes it will respond.
Sometimes it will wait.`,

    `Silence is not an error.

Nexus may wait because it is uncertain,
or because it thinks waiting is better than guessing.

You don't need to fill the silence.`,

    `You can interrupt Nexus at any time.

Speaking will immediately stop it.

You don't need to say "stop."`,

    `Nexus forgets by default.

It may remember patterns over time,
but it will ask before keeping anything important.

It can correct itself.`
];

const OnboardingManager = {
    currentScreen: 0,

    async check() {
        const completed = await invoke('get_onboarding_status');
        return completed;
    },

    start() {
        console.log('[Onboarding] Starting');
        this.currentScreen = 0;
        dom.onboardingOverlay.style.display = 'flex';
        this.render();
    },

    render() {
        console.log(`[Onboarding] Rendering screen ${this.currentScreen}`);
        onboardingText.textContent = ONBOARDING_SCREENS[this.currentScreen];
        // Final screen has "Begin" button
        if (this.currentScreen === ONBOARDING_SCREENS.length - 1) {
            onboardingButton.innerText = 'Begin';
        } else {
            onboardingButton.innerText = 'Continue';
        }
    },

    next() {
        console.log('[Onboarding] Next called');
        this.currentScreen++;
        if (this.currentScreen >= ONBOARDING_SCREENS.length) {
            this.finish();
        } else {
            this.render();
        }
    },

    async finish() {
        // Call Driver to persist and unlock kernel
        await invoke('complete_onboarding');
        dom.onboardingOverlay.style.display = 'none';
        // Now trigger normal UI attach
        invoke('ui_attach');
        dom.mic.click(); // Default mic ON
    }
};

// --- 1. Event Stream (Core -> UI) ---
listen('nexus-event', (event) => {
    const payload = event.payload;

    switch (payload.type) {
        case 'PresenceUpdate':
            updatePresence(payload.state);
            break;
        case 'OutputEvent':
            renderOutput(payload);
            break;
        case 'ContextSnapshot':
            hydrateContext(payload.content); // Push-based Hydration
            break;
        case 'InputAck':
            // Optional: Core acknowledging receipt of input.
            // Useful if we want to "solidify" the user's view of their own typing.
            // For now, ambient input is fire-and-forget streaming.
            break;
    }
});

// --- 2. Presence Handling (Strict) ---
function updatePresence(state) {
    // RESET existing classes to ensure clean transition
    dom.body.className = '';

    // Set Text Content based on State
    // User Rule: "Make it textual, not symbolic. Or almost invisible."
    let text = "";
    switch (state) {
        case 'Dormant':
            text = "";
            dom.body.classList.add('state-dormant');
            break;
        case 'Attentive':
            text = "Listening";
            dom.body.classList.add('state-attentive');
            break;
        case 'Engaged':
            text = "Active";
            dom.body.classList.add('state-engaged');
            break;
        case 'QuietlyHolding':
            text = "Holding";
            dom.body.classList.add('state-holding');
            break;
        case 'Suspended':
            text = "Paused";
            dom.body.classList.add('state-suspended');
            break;
        default:
            text = "";
    }
    dom.indicator.textContent = text;
}

// --- 3. Output Rendering (The Canvas) ---
let currentDraftSpan = null;

function renderOutput(payload) {
    // payload: { content: string, status: 'Draft' | 'SoftCommit' | 'HardCommit' }

    if (payload.status === 'Draft') {
        // Update or Create Draft
        if (!currentDraftSpan) {
            currentDraftSpan = document.createElement('span');
            currentDraftSpan.className = 'fragment draft';
            dom.canvas.appendChild(currentDraftSpan);
        }
        currentDraftSpan.textContent = payload.content;
        scrollToBottom();
    }
    else if (payload.status === 'SoftCommit' || payload.status === 'HardCommit') {
        // Finalize
        if (currentDraftSpan) {
            // If the draft matches the commit, convert it. 
            // Else, remove draft and append commit (Correction case).
            currentDraftSpan.className = 'fragment committed';
            currentDraftSpan.textContent = payload.content; // Authoritative replacement
            currentDraftSpan = null; // Clear draft reference
        } else {
            // New commit without draft (e.g. system message)
            const span = document.createElement('span');
            span.className = 'fragment committed';
            span.textContent = payload.content;
            dom.canvas.appendChild(span);
        }
        scrollToBottom();
    }
}

function scrollToBottom() {
    dom.canvas.scrollTop = dom.canvas.scrollHeight;
}

// --- 4. Context Hydration (Push-Based) ---
function hydrateContext(history) {
    // history: Array of { content: string, role: string }
    dom.canvas.innerHTML = ''; // Wipe canvas (Rule: UI has no memory)
    currentDraftSpan = null;

    history.forEach(item => {
        const span = document.createElement('span');
        // Simple mapping for Phase C
        span.className = item.role === 'user' ? 'fragment user-input' : 'fragment committed';
        span.textContent = item.content;
        dom.canvas.appendChild(span);
    });
    scrollToBottom();
}

// --- 5. Ambient Input Handling (UI -> Core) ---
// Note: We do NOT render user input directly in the canvas as "Committed".
// We rely on the Core to echo it back OR we render it as "Provisional" via `InputAck`.
// For Phase C "No Persistence" rule, let's treat user typing as a stream.
// To make it usable, we show what you type, but it is NOT persisted.

dom.input.addEventListener('input', (e) => {
    const text = e.target.value;

    // 1. Send Fragment to Core
    invoke('send_input_fragment', { text: text });

    // 2. Clear input? No, streaming.
    // Actually, "Input is fragmentary".
    // If we want "Google Search" style instant typing:
    // We send char-by-char.
    // The Input Box is invisible, so where does the user see it?
    // "Thinking Canvas" -> User text appears on canvas.
    // Implementation: We append a specific "User Draft" span that mirrors input val.
});

// Mic Toggle
dom.mic.addEventListener('click', () => {
    dom.mic.classList.toggle('active');
    const isActive = dom.mic.classList.contains('active');
    invoke('toggle_mic', { active: isActive });
});

// --- 6. Initialization ---
window.addEventListener('DOMContentLoaded', async () => {
    // Ensure input has focus
    dom.input.focus();

    // Listen for Access Denied (Strict Gate)
    listen('access-denied', () => {
        console.log('[Access] Denied.');
        document.getElementById('access-denied-screen').style.display = 'flex';
        // Hide everything else
        dom.canvas.style.display = 'none';
        dom.input.disabled = true;
    });

    // Handle Boot Flow
    // Phase K: Check Onboarding Status
    const onboardingComplete = await OnboardingManager.check();
    console.log('[Init] Onboarding Complete:', onboardingComplete);

    if (!onboardingComplete) {
        OnboardingManager.start();
        onboardingButton.addEventListener('click', (e) => {
            e.preventDefault();
            e.stopPropagation();
            OnboardingManager.next();
        });
    } else {
        // Phase M: One-Time Welcome Check
        // We need to know if welcome was shown. 
        // We reuse the onboarding state for this, or check a separate flag?
        // Current Main.rs `get_onboarding_status` returns bool.
        // We need `get_full_onboarding_state` or assume we just show it if we haven't seen it in session? 
        // No, persistent.
        // Let's implement `mark_welcome_seen` in main.rs and a check here.
        // Updating main.rs to expose welcome_shown is needed, or we just call a command.

        // Command: `check_welcome_needed` -> bool
        // If true -> Show Intro -> On Click -> `mark_welcome_seen`

        // Since I didn't add `check_welcome_needed` command specifically yet, I'll rely on `invoke` returning the struct if I update the command, 
        // OR I update main.rs to add that command now.
        // WAIT. I did update `OnboardingState` struct, but `get_onboarding_status` returns bool.
        // I should update main.rs to expose the full state or a specific check.
        // For now, I will assume I can update `get_onboarding_status` or add a new one.
        // Let's assume I add `should_show_welcome` to main.rs. I'll add that in the next step to be safe.
        // Or I can update `get_onboarding_status` to return object?
        // Stick to plan: "Update shell/renderer.js: Logic for Access Gate & One-Time Welcome"
        // I will assume `should_show_welcome` command exists or I'll add it.
        // Let's implement the JS assuming it exists, then go fix Main.rs if I missed it.

        // Actually, looking at main.rs, I only updated the struct. `get_onboarding_status` returns `bool`.
        // I need to update main.rs to return the full state or a specific flag.
        // I will do that in the next turn (Driver Update Part 2).

        // For now, write the JS logic assuming `should_show_welcome` returns true/false.

        const showWelcome = await invoke('should_show_welcome').catch(() => false); // Fallback false if command missing

        if (showWelcome) {
            const intro = document.getElementById('intro-screen');
            const beginBtn = document.getElementById('intro-begin');
            intro.style.display = 'flex';

            beginBtn.onclick = async () => {
                await invoke('mark_welcome_seen');
                intro.style.display = 'none';
                startKeySystems();
            };
        } else {
            startKeySystems();
        }
    }
});

function startKeySystems() {
    invoke('ui_attach');
    // Default Mic to ON for Phase D/M testing (unless changed by user preference later)
    dom.mic.click();
}
// --- Phase L: Memory Consent ---
const ConsentManager = {
    overlay: document.getElementById('memory-consent-container'),
    yesBtn: document.getElementById('consent-yes'),
    noBtn: document.getElementById('consent-no'),
    currentKey: null,
    timer: null,

    init() {
        if (this.yesBtn) this.yesBtn.onclick = () => this.resolve('granted');
        if (this.noBtn) this.noBtn.onclick = () => this.resolve('declined');

        listen('ask-memory-consent', (event) => {
            console.log('[Consent] Asked for key:', event.payload.key);
            this.show(event.payload.key);
        });
    },

    show(key) {
        if (this.timer) clearTimeout(this.timer);
        this.currentKey = key;

        // Render UI
        this.overlay.classList.remove('consent-hidden');

        // Auto-dismiss (Ignored)
        // 10 seconds timeout
        this.timer = setTimeout(() => {
            console.log('[Consent] Timed out -> Ignored');
            this.resolve('ignored');
        }, 10000);
    },

    hide() {
        this.overlay.classList.add('consent-hidden');
        if (this.timer) clearTimeout(this.timer);
        this.currentKey = null;
    },

    resolve(state) {
        if (!this.currentKey) return;

        // Invoke Kernel Command
        // We need to serialize key back to string if it is an object
        const keyJson = JSON.stringify(this.currentKey);

        invoke('resolve_memory_consent', { keyJson, state })
            .catch(err => console.error('[Consent] Failed to resolve:', err));

        console.log(`[Consent] Resolved: ${state}`);
        this.hide();
    }
};

// Start logic
ConsentManager.init();
