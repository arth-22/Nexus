const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;

const indicator = document.getElementById('presence-indicator');
const output = document.getElementById('output-area');

// Listen for Presence Updates
listen('nexus-event', (event) => {
    const payload = event.payload;

    // Core -> UI: Presence Update
    if (payload.type === 'PresenceUpdate') {
        updatePresence(payload.state);
    }
    // Core -> UI: Output Event
    else if (payload.type === 'OutputEvent') {
        renderOutput(payload.content);
    }
});

function updatePresence(state) {
    // Reset classes
    indicator.className = '';

    // Exact mapping to CSS classes
    // Dormant, Attentive, Engaged, QuietlyHolding, Suspended
    switch (state) {
        case 'Dormant': indicator.classList.add('state-dormant'); break;
        case 'Attentive': indicator.classList.add('state-attentive'); break;
        case 'Engaged': indicator.classList.add('state-engaged'); break;
        case 'QuietlyHolding': indicator.classList.add('state-holding'); break;
        case 'Suspended': indicator.classList.add('state-dormant'); break;
    }
}

function renderOutput(text) {
    output.textContent = text;
}

// Minimal Input Capture (Placeholder for Phase B)
// In Phase A/B we don't have full audio, just text stub or keys.
document.addEventListener('keydown', (e) => {
    // Send generic activity signal to Core
    // UI cannot force state, only report activity.
    invoke('send_input_signal', { signal: 'Activity' });
});
