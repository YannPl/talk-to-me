import * as api from './api.js';

const overlay = document.getElementById('overlay');
const modeStt = document.getElementById('mode-stt');
const modeTranscribing = document.getElementById('mode-transcribing');
const modeTts = document.getElementById('mode-tts');
const sttStatus = document.getElementById('stt-status');
const bars = document.querySelectorAll('.visualizer .bar');

function showMode(mode) {
    modeStt.classList.add('hidden');
    modeTranscribing.classList.add('hidden');
    modeTts.classList.add('hidden');

    switch (mode) {
        case 'recording':
            modeStt.classList.remove('hidden');
            sttStatus.textContent = 'Listening...';
            overlay.classList.add('visible');
            break;
        case 'transcribing':
            modeTranscribing.classList.remove('hidden');
            overlay.classList.add('visible');
            break;
        case 'tts':
            modeTts.classList.remove('hidden');
            overlay.classList.add('visible');
            break;
        case 'idle':
            overlay.classList.remove('visible');
            break;
    }
}

// Listen for recording status changes
api.onRecordingStatus((data) => {
    showMode(data.status);
});

// Listen for audio level (update visualizer bars)
api.onAudioLevel((data) => {
    const level = data.level;
    bars.forEach((bar, i) => {
        // Create a somewhat random-looking visualization based on the level
        const offset = Math.sin(Date.now() / 200 + i * 0.5) * 0.3;
        const height = Math.max(4, (level + offset) * 48);
        bar.style.height = `${height}px`;
    });
});

// Listen for overlay mode (STT vs TTS)
api.onOverlayMode((data) => {
    if (data.mode === 'tts') {
        showMode('tts');
    }
});

// Listen for transcription complete — hide overlay
api.onTranscriptionComplete(() => {
    setTimeout(() => showMode('idle'), 500);
});

// TTS playback progress
api.onPlaybackProgress((data) => {
    const fill = document.getElementById('tts-progress');
    if (fill) fill.style.width = `${data.progress * 100}%`;
});

api.onPlaybackStatus((data) => {
    if (data.status === 'idle') {
        showMode('idle');
    }
});
