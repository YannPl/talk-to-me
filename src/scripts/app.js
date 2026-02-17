import * as api from './api.js';

// Tab switching
document.querySelectorAll('.tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
        tab.classList.add('active');
        document.getElementById(`tab-${tab.dataset.tab}`).classList.add('active');
    });
});

// Load settings on startup
async function loadSettings() {
    try {
        const settings = await api.getSettings();
        document.getElementById('language-select').value = settings.stt.language;
        document.getElementById('injection-mode').value = settings.stt.injection_mode;
        document.getElementById('launch-at-login').checked = settings.general.launch_at_login;
        document.getElementById('sound-feedback').checked = settings.general.sound_feedback;
    } catch (e) {
        console.error('Failed to load settings:', e);
    }
}

// Save settings on change
async function saveSettings() {
    try {
        const settings = {
            shortcuts: {
                stt: document.getElementById('stt-shortcut').textContent.trim(),
                tts: 'Option+Shift+Space',
            },
            stt: {
                language: document.getElementById('language-select').value,
                injection_mode: document.getElementById('injection-mode').value,
                active_model_id: null, // managed separately
            },
            tts: {
                active_model_id: null,
                speed: 1.0,
                voice_id: null,
            },
            general: {
                launch_at_login: document.getElementById('launch-at-login').checked,
                sound_feedback: document.getElementById('sound-feedback').checked,
            },
        };
        await api.updateSettings(settings);
    } catch (e) {
        console.error('Failed to save settings:', e);
    }
}

// Bind change events to save
['language-select', 'injection-mode'].forEach(id => {
    document.getElementById(id).addEventListener('change', saveSettings);
});
['launch-at-login', 'sound-feedback'].forEach(id => {
    document.getElementById(id).addEventListener('change', saveSettings);
});

// Check accessibility
async function checkAccessibility() {
    try {
        const granted = await api.checkAccessibilityPermission();
        const statusEl = document.getElementById('accessibility-status');
        const btnEl = document.getElementById('request-accessibility');
        if (granted) {
            statusEl.textContent = '\u2713 Accessibility permission granted';
            statusEl.classList.add('status-ok');
            btnEl.style.display = 'none';
        } else {
            statusEl.textContent = '\u26A0 Accessibility permission required';
            statusEl.classList.add('status-warn');
            btnEl.style.display = 'inline-block';
        }
    } catch (e) {
        console.error('Failed to check accessibility:', e);
    }
}

document.getElementById('request-accessibility')?.addEventListener('click', async () => {
    await api.requestAccessibilityPermission();
});

// Load model catalog
async function loadCatalog() {
    try {
        const catalog = await api.getCatalog('stt');
        const container = document.getElementById('model-catalog');
        container.innerHTML = '';

        for (const model of catalog) {
            const sizeStr = formatSize(model.files.reduce((sum, f) => sum + f.size_bytes, 0));
            const langStr = model.languages.join(', ');

            const card = document.createElement('div');
            card.className = 'model-card';
            card.innerHTML = `
                <div class="model-info">
                    <div class="model-name">${model.name}</div>
                    <div class="model-desc">${model.description || ''}</div>
                    <div class="model-meta">
                        <span class="model-size">${sizeStr}</span>
                        <span class="model-lang">${langStr}</span>
                    </div>
                </div>
                <button class="btn-download" data-model-id="${model.id}">\u2B07 Download</button>
            `;
            container.appendChild(card);
        }

        // Bind download buttons
        container.querySelectorAll('.btn-download').forEach(btn => {
            btn.addEventListener('click', () => downloadModel(btn.dataset.modelId));
        });
    } catch (e) {
        console.error('Failed to load catalog:', e);
    }
}

// Load installed models
async function loadInstalled() {
    try {
        const models = await api.listInstalledModels('stt');
        const container = document.getElementById('installed-models');

        if (models.length === 0) {
            container.innerHTML = '<p class="empty-state">No models installed. Download one below to get started.</p>';
            return;
        }

        const activeId = await api.getActiveModel('stt');
        container.innerHTML = '';

        for (const model of models) {
            const card = document.createElement('div');
            card.className = 'model-card installed';
            card.innerHTML = `
                <div class="model-select">
                    <input type="radio" name="active-stt" value="${model.id}" ${model.id === activeId ? 'checked' : ''}>
                </div>
                <div class="model-info">
                    <div class="model-name">${model.name}</div>
                    <div class="model-meta">
                        <span class="model-size">${formatSize(model.size_bytes)}</span>
                    </div>
                </div>
                <button class="btn-delete" data-model-id="${model.id}" title="Delete">\uD83D\uDDD1</button>
            `;
            container.appendChild(card);
        }

        // Bind radio buttons
        container.querySelectorAll('input[name="active-stt"]').forEach(radio => {
            radio.addEventListener('change', () => api.setActiveModel(radio.value, 'stt'));
        });

        // Bind delete buttons
        container.querySelectorAll('.btn-delete').forEach(btn => {
            btn.addEventListener('click', async () => {
                if (confirm('Delete this model?')) {
                    await api.deleteModel(btn.dataset.modelId);
                    loadInstalled();
                }
            });
        });
    } catch (e) {
        console.error('Failed to load installed models:', e);
    }
}

async function downloadModel(modelId) {
    const progressEl = document.getElementById('download-progress');
    const nameEl = document.getElementById('download-model-name');
    const statsEl = document.getElementById('download-stats');
    const fillEl = document.getElementById('progress-fill');

    nameEl.textContent = `Downloading ${modelId}...`;
    progressEl.style.display = 'flex';
    fillEl.style.width = '0%';

    try {
        await api.downloadModel(modelId);
        // Command completed successfully — hide progress and refresh lists
        progressEl.style.display = 'none';
        await loadInstalled();
        await loadCatalog();
    } catch (e) {
        console.error('Download failed:', e);
        progressEl.style.display = 'none';
    }
}

// Download progress listener
api.onDownloadProgress((data) => {
    const fillEl = document.getElementById('progress-fill');
    const statsEl = document.getElementById('download-stats');
    fillEl.style.width = `${(data.progress * 100).toFixed(1)}%`;
    statsEl.textContent = `${formatSize(data.speed_bps)}/s \u2014 ${formatTime(data.eta_seconds)} remaining`;
});

api.onDownloadComplete(() => {
    document.getElementById('download-progress').style.display = 'none';
    loadInstalled();
    loadCatalog();
});

api.onDownloadError((data) => {
    document.getElementById('download-progress').style.display = 'none';
    alert(`Download failed: ${data.error}`);
});

// Utility functions
function formatSize(bytes) {
    if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
    if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(0)} MB`;
    if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(0)} KB`;
    return `${bytes} B`;
}

function formatTime(seconds) {
    if (seconds >= 3600) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
    if (seconds >= 60) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    return `${seconds}s`;
}

// App version
async function loadVersion() {
    try {
        const version = await api.getAppVersion();
        document.getElementById('app-version').textContent = `Talk to Me v${version}`;
    } catch (e) {
        // ignore
    }
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
    loadSettings();
    checkAccessibility();
    loadCatalog();
    loadInstalled();
    loadVersion();
});
