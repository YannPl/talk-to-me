import * as api from './api.js';

const overlay = document.getElementById('overlay');
const modeStt = document.getElementById('mode-stt');
const modeTranscribing = document.getElementById('mode-transcribing');
const modeTts = document.getElementById('mode-tts');
const modeLoading = document.getElementById('mode-loading');
const sttStatus = document.getElementById('stt-status');

const BAR_COUNT = 48;
const visualizer = document.getElementById('audio-visualizer');
for (let i = 0; i < BAR_COUNT; i++) {
    const bar = document.createElement('div');
    bar.className = 'bar';
    const t = i / (BAR_COUNT - 1);
    bar.style.animationDelay = `${(0.5 - Math.abs(t - 0.5)) * 1.2}s`;
    visualizer.appendChild(bar);
}
const bars = visualizer.querySelectorAll('.bar');

let smoothLevel = 0;
let previousMode = 'idle';

function resetBars() {
    smoothLevel = 0;
    bars.forEach(bar => bar.style.removeProperty('height'));
}

function showMode(mode) {
    previousMode = mode;

    modeStt.classList.add('hidden');
    modeLoading.classList.add('hidden');
    modeTranscribing.classList.add('hidden');
    modeTts.classList.add('hidden');

    switch (mode) {
        case 'loading':
            modeLoading.classList.remove('hidden');
            overlay.classList.add('visible');
            break;
        case 'recording':
            modeStt.classList.remove('hidden');
            sttStatus.textContent = 'Listening...';
            overlay.classList.add('visible');
            break;
        case 'transcribing':
            resetBars();
            document.getElementById('transcription-progress').style.removeProperty('width');
            modeTranscribing.classList.remove('hidden');
            overlay.classList.add('visible');
            break;
        case 'tts':
            modeTts.classList.remove('hidden');
            overlay.classList.add('visible');
            break;
        case 'idle':
            resetBars();
            overlay.classList.remove('visible');
            break;
    }
}

api.onRecordingStatus((data) => {
    showMode(data.status);
});


api.onAudioLevel((data) => {
    const alpha = data.level > smoothLevel ? 0.5 : 0.15;
    smoothLevel += (data.level - smoothLevel) * alpha;
    const now = Date.now();
    bars.forEach((bar, i) => {
        const w1 = Math.sin(now / 150 + i * 0.7) * 0.25;
        const w2 = Math.sin(now / 300 + i * 1.3) * 0.15;
        const h = Math.max(4, (smoothLevel + (w1 + w2) * smoothLevel) * 48);
        bar.style.height = `${h}px`;
    });
});

api.onOverlayMode((data) => {
    if (data.mode === 'tts') {
        showMode('tts');
    }
});

api.onStreamingTranscription((data) => {
    if (previousMode === 'recording' && data.chunks_completed > 0) {
        sttStatus.textContent = `Listening... (${data.chunks_completed} chunk${data.chunks_completed > 1 ? 's' : ''} ready)`;
    }
});

api.onTranscriptionProgress((data) => {
    const fill = document.getElementById('transcription-progress');
    if (fill && data.total > 1) {
        fill.style.width = `${(data.chunk / data.total) * 100}%`;
    }
});

api.onTranscriptionComplete(() => {
    setTimeout(() => showMode('idle'), 500);
});

api.onPlaybackProgress((data) => {
    const fill = document.getElementById('tts-progress');
    if (fill) fill.style.width = `${data.progress * 100}%`;
});

api.onPlaybackStatus((data) => {
    if (data.status === 'idle') {
        showMode('idle');
    }
});
