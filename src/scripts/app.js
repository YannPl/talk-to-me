import * as api from './api.js';

function switchTab(tabName) {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    const tabBtn = document.querySelector(`.tab[data-tab="${tabName}"]`);
    if (tabBtn) tabBtn.classList.add('active');
    const tabContent = document.getElementById(`tab-${tabName}`);
    if (tabContent) tabContent.classList.add('active');
}

document.querySelectorAll('.tab').forEach(tab => {
    tab.addEventListener('click', () => switchTab(tab.dataset.tab));
});

api.onNavigateTab((tabName) => switchTab(tabName));

async function loadSettings() {
    try {
        const settings = await api.getSettings();
        document.getElementById('language-select').value = settings.stt.language;
        document.getElementById('injection-mode').value = settings.stt.injection_mode;
        document.getElementById('recording-mode').value = settings.stt.recording_mode || 'toggle';
        document.getElementById('launch-at-login').checked = settings.general.launch_at_login;
        document.getElementById('sound-feedback').checked = settings.general.sound_feedback;
    } catch (e) {
        console.error('Failed to load settings:', e);
    }
}

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
                recording_mode: document.getElementById('recording-mode').value,
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

['language-select', 'injection-mode', 'recording-mode'].forEach(id => {
    document.getElementById(id).addEventListener('change', saveSettings);
});
['launch-at-login', 'sound-feedback'].forEach(id => {
    document.getElementById(id).addEventListener('change', saveSettings);
});

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

async function loadCatalog() {
    try {
        const catalog = await api.getCatalog('stt');
        const installed = await api.listInstalledModels('stt');
        const installedIds = new Set(installed.map(m => m.id));
        const container = document.getElementById('model-catalog');
        container.innerHTML = '';

        for (const model of catalog) {
            if (installedIds.has(model.id)) continue;

            const sizeStr = formatSize(model.files.reduce((sum, f) => sum + f.size_bytes, 0));
            const langStr = model.languages.join(', ');

            const card = document.createElement('div');
            card.className = 'model-card';
            card.dataset.modelId = model.id;

            const infoDiv = document.createElement('div');
            infoDiv.className = 'model-info';
            infoDiv.innerHTML = `
                <div class="model-name">${model.name}</div>
                <div class="model-desc">${model.description || ''}</div>
                <div class="model-meta">
                    <span class="model-size">${sizeStr}</span>
                    <span class="model-lang">${langStr}</span>
                </div>
            `;

            const actionDiv = document.createElement('div');
            actionDiv.className = 'model-action';

            if (activeDownloads.has(model.id)) {
                actionDiv.classList.add('downloading');

                const progressDiv = document.createElement('div');
                progressDiv.className = 'inline-progress';
                progressDiv.innerHTML = `
                    <div class="inline-progress-bar"><div class="inline-progress-fill"></div></div>
                    <span class="inline-progress-text">0%</span>
                `;

                const cancelBtn = document.createElement('button');
                cancelBtn.className = 'btn-cancel';
                cancelBtn.title = 'Cancel download';
                cancelBtn.textContent = '\u2715';
                cancelBtn.addEventListener('click', (e) => {
                    e.stopPropagation();
                    cancelDownload(model.id);
                });

                actionDiv.appendChild(progressDiv);
                actionDiv.appendChild(cancelBtn);
            } else {
                const btn = document.createElement('button');
                btn.className = 'btn-download';
                btn.textContent = '\u2B07 Download';
                btn.addEventListener('click', () => downloadModel(model.id));
                actionDiv.appendChild(btn);
            }

            card.appendChild(infoDiv);
            card.appendChild(actionDiv);
            container.appendChild(card);
        }

        if (container.children.length === 0) {
            container.innerHTML = '<p class="empty-state">All available models are installed.</p>';
        }
    } catch (e) {
        console.error('Failed to load catalog:', e);
    }
}

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

            const selectDiv = document.createElement('div');
            selectDiv.className = 'model-select';
            const radio = document.createElement('input');
            radio.type = 'radio';
            radio.name = 'active-stt';
            radio.value = model.id;
            radio.checked = model.id === activeId;
            selectDiv.appendChild(radio);

            const infoDiv = document.createElement('div');
            infoDiv.className = 'model-info';
            infoDiv.innerHTML = `
                <div class="model-name">${model.name}</div>
                <div class="model-meta">
                    <span class="model-size">${formatSize(model.size_bytes)}</span>
                </div>
            `;

            const deleteBtn = document.createElement('button');
            deleteBtn.className = 'btn-delete';
            deleteBtn.title = 'Delete';
            deleteBtn.textContent = '\u2715';
            deleteBtn.addEventListener('click', async (e) => {
                e.preventDefault();
                e.stopPropagation();
                const confirmed = await showConfirm(`Delete ${model.name}?`);
                if (!confirmed) return;
                try {
                    await api.deleteModel(model.id);
                    await loadInstalled();
                    await loadCatalog();
                } catch (err) {
                    console.error('Failed to delete model:', err);
                }
            });

            radio.addEventListener('change', () => api.setActiveModel(model.id, 'stt'));

            card.appendChild(selectDiv);
            card.appendChild(infoDiv);
            card.appendChild(deleteBtn);
            container.appendChild(card);
        }
    } catch (e) {
        console.error('Failed to load installed models:', e);
    }
}

const activeDownloads = new Set();

async function downloadModel(modelId) {
    if (activeDownloads.has(modelId)) return; // already downloading
    activeDownloads.add(modelId);

    await loadCatalog();

    try {
        await api.downloadModel(modelId);
        activeDownloads.delete(modelId);
        await loadInstalled();
        await loadCatalog();
    } catch (e) {
        activeDownloads.delete(modelId);
        if (e !== 'cancelled') {
            console.error('Download failed:', e);
        }
        await loadCatalog();
    }
}

async function cancelDownload(modelId) {
    try {
        await api.cancelDownload(modelId);
    } catch (e) {
        console.error('Failed to cancel download:', e);
    }
}

function updateInlineProgress(modelId, progress, speedBps, etaSeconds) {
    const card = document.querySelector(`.model-card[data-model-id="${CSS.escape(modelId)}"]`);
    if (!card) return;
    const fill = card.querySelector('.inline-progress-fill');
    const text = card.querySelector('.inline-progress-text');
    if (fill) fill.style.width = `${(progress * 100).toFixed(1)}%`;
    if (text) text.textContent = `${(progress * 100).toFixed(0)}% \u2014 ${formatSize(speedBps)}/s`;
}

api.onDownloadProgress((data) => {
    updateInlineProgress(data.model_id, data.progress, data.speed_bps, data.eta_seconds);
});

api.onDownloadComplete((data) => {
    const modelId = data?.model_id;
    if (modelId) activeDownloads.delete(modelId);
    loadInstalled();
    loadCatalog();
});

api.onDownloadError((data) => {
    const modelId = data?.model_id;
    if (modelId) activeDownloads.delete(modelId);
    loadCatalog();
});

// Native confirm/alert is blocked in Tauri webview
function showConfirm(message) {
    return new Promise((resolve) => {
        const overlay = document.createElement('div');
        overlay.className = 'confirm-overlay';
        overlay.innerHTML = `
            <div class="confirm-dialog">
                <p>${message}</p>
                <div class="confirm-actions">
                    <button class="btn-confirm-cancel">Cancel</button>
                    <button class="btn-confirm-delete">Delete</button>
                </div>
            </div>
        `;
        document.body.appendChild(overlay);

        overlay.querySelector('.btn-confirm-cancel').addEventListener('click', () => {
            overlay.remove();
            resolve(false);
        });
        overlay.querySelector('.btn-confirm-delete').addEventListener('click', () => {
            overlay.remove();
            resolve(true);
        });
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) {
                overlay.remove();
                resolve(false);
            }
        });
    });
}

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

async function loadVersion() {
    try {
        const version = await api.getAppVersion();
        document.getElementById('app-version').textContent = `Talk to Me v${version}`;
    } catch (e) {
        // ignore
    }
}

document.addEventListener('DOMContentLoaded', () => {
    loadSettings();
    checkAccessibility();
    loadCatalog();
    loadInstalled();
    loadVersion();
});
