import * as api from './api.js';

const RECOMMENDED_MODEL = 'openai/whisper-large-v3-turbo';
let currentStep = 1;
let accessibilityPollingId = null;
let installedModelId = null;
const activeDownloads = new Set();

function formatSize(bytes) {
    if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
    if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(0)} MB`;
    if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(0)} KB`;
    return `${bytes} B`;
}

function goToStep(step) {
    const previousStep = currentStep;

    document.querySelectorAll('.wizard-step').forEach(el => el.classList.remove('active'));
    const targetSection = document.getElementById(
        ['step-welcome', 'step-model', 'step-accessibility', 'step-test'][step - 1]
    );
    if (targetSection) targetSection.classList.add('active');

    document.querySelectorAll('.progress-step').forEach(el => {
        const s = parseInt(el.dataset.step, 10);
        el.classList.remove('active', 'completed');
        if (s < step) {
            el.classList.add('completed');
        } else if (s === step) {
            el.classList.add('active');
        }
    });

    const lines = document.querySelectorAll('.progress-line');
    lines.forEach((line, i) => {
        const lineAfterStep = i + 1;
        if (lineAfterStep < step) {
            line.classList.add('completed');
        } else {
            line.classList.remove('completed');
        }
    });

    currentStep = step;

    if (previousStep === 3 && step !== 3) {
        stopAccessibilityPolling();
    }

    if (step === 2) {
        loadCatalog();
    } else if (step === 3) {
        startAccessibilityPolling();
        loadCurrentShortcut();
    } else if (step === 4) {
        enterTestStep();
    }
}

async function loadCurrentShortcut() {
    try {
        const settings = await api.getSettings();
        const shortcutEl = document.getElementById('onboarding-shortcut');
        shortcutEl.value = settings.shortcuts.stt || 'Alt+Space';
        shortcutEl.dataset.previousValue = shortcutEl.value;
    } catch (e) {
        console.error('Failed to load shortcut setting:', e);
    }
}

async function loadCatalog() {
    try {
        const [catalog, installed] = await Promise.all([
            api.getCatalog('stt'),
            api.listInstalledModels('stt'),
        ]);

        const installedIds = new Set(installed.map(m => m.id));
        const container = document.getElementById('onboarding-catalog');
        const installedContainer = document.getElementById('onboarding-installed');
        const nextBtn = document.getElementById('btn-model-next');

        container.innerHTML = '';
        installedContainer.innerHTML = '';

        if (installed.length > 0) {
            installedModelId = installed[0].id;
            const activeId = await api.getActiveModel('stt');
            installedContainer.className = 'installed-status ready';
            const activeName = installed.find(m => m.id === activeId)?.name || installed[0].name;
            installedContainer.textContent = `${activeName} is ready to use`;
            nextBtn.disabled = false;
        } else {
            installedModelId = null;
            installedContainer.className = 'installed-status';
            installedContainer.textContent = '';
            nextBtn.disabled = true;
        }

        for (const model of catalog) {
            if (installedIds.has(model.id)) continue;

            const totalSize = model.files.reduce((sum, f) => sum + f.size_bytes, 0);
            const sizeStr = formatSize(totalSize);
            const langStr = model.languages ? model.languages.join(', ') : '';

            const card = document.createElement('div');
            card.className = 'model-card';
            card.dataset.modelId = model.id;

            const infoDiv = document.createElement('div');
            infoDiv.className = 'model-info';

            const nameHtml = model.id === RECOMMENDED_MODEL
                ? `${model.name} <span class="badge-recommended">Recommended</span>`
                : model.name;

            infoDiv.innerHTML = `
                <div class="model-name">${nameHtml}</div>
                <div class="model-desc">${model.description || ''}</div>
                <div class="model-meta">
                    <span class="model-size">${sizeStr}</span>
                    ${langStr ? `<span class="model-lang">${langStr}</span>` : ''}
                </div>
            `;

            const actionDiv = document.createElement('div');
            actionDiv.className = 'model-action';

            if (activeDownloads.has(model.id)) {
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
                btn.addEventListener('click', () => startDownload(model.id));
                actionDiv.appendChild(btn);
            }

            card.appendChild(infoDiv);
            card.appendChild(actionDiv);
            container.appendChild(card);
        }

        if (container.children.length === 0 && installed.length > 0) {
            container.innerHTML = '<p class="empty-state">All available models are installed.</p>';
        }
    } catch (e) {
        console.error('Failed to load catalog:', e);
    }
}

async function startDownload(modelId) {
    if (activeDownloads.has(modelId)) return;
    activeDownloads.add(modelId);
    await loadCatalog();

    try {
        await api.downloadModel(modelId);
        activeDownloads.delete(modelId);
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

function updateInlineProgress(modelId, progress, speedBps) {
    const card = document.querySelector(`.model-card[data-model-id="${CSS.escape(modelId)}"]`);
    if (!card) return;
    const fill = card.querySelector('.inline-progress-fill');
    const text = card.querySelector('.inline-progress-text');
    if (fill) fill.style.width = `${(progress * 100).toFixed(1)}%`;
    if (text) text.textContent = `${(progress * 100).toFixed(0)}% \u2014 ${formatSize(speedBps)}/s`;
}

api.onDownloadProgress((data) => {
    updateInlineProgress(data.model_id, data.progress, data.speed_bps);
});

api.onDownloadComplete(async (data) => {
    const modelId = data?.model_id;
    if (modelId) {
        activeDownloads.delete(modelId);
        try {
            await api.setActiveModel(modelId, 'stt');
        } catch (e) {
            console.error('Failed to set active model:', e);
        }
    }
    await loadCatalog();
});

api.onDownloadError((data) => {
    const modelId = data?.model_id;
    if (modelId) activeDownloads.delete(modelId);
    loadCatalog();
});

function startAccessibilityPolling() {
    checkAccessibilityNow();
    accessibilityPollingId = setInterval(checkAccessibilityNow, 2000);
}

function stopAccessibilityPolling() {
    if (accessibilityPollingId) {
        clearInterval(accessibilityPollingId);
        accessibilityPollingId = null;
    }
}

async function checkAccessibilityNow() {
    try {
        const granted = await api.checkAccessibilityPermission();
        const indicator = document.getElementById('accessibility-indicator');
        const grantBtn = document.getElementById('btn-grant-accessibility');
        const skipHint = document.getElementById('accessibility-skip-hint');
        const skipBtn = document.getElementById('btn-access-skip');

        const nextBtn = document.getElementById('btn-access-next');

        if (granted) {
            indicator.textContent = 'Accessibility granted';
            indicator.className = 'permission-indicator granted';
            grantBtn.style.display = 'none';
            skipHint.style.display = 'none';
            skipBtn.style.display = 'none';
            nextBtn.disabled = false;
        } else {
            indicator.textContent = 'Permission required';
            indicator.className = 'permission-indicator denied';
            grantBtn.style.display = 'inline-block';
            skipHint.style.display = 'block';
            skipBtn.style.display = 'inline-block';
            nextBtn.disabled = true;
        }
    } catch (e) {
        console.error('Failed to check accessibility:', e);
    }
}

async function enterTestStep() {
    try {
        const label = await api.getSttShortcutLabel();
        document.getElementById('test-shortcut-label').textContent = label;
    } catch (e) {
        console.error('Failed to get shortcut label:', e);
    }

    const status = document.getElementById('test-status');
    status.textContent = 'Ready';
    status.className = 'test-status';
    document.getElementById('test-result').textContent = '';
    document.getElementById('mic-error').style.display = 'none';
}

api.onRecordingStatus((data) => {
    const status = document.getElementById('test-status');
    const micError = document.getElementById('mic-error');
    const micErrorText = document.getElementById('mic-error-text');

    if (data.error) {
        micErrorText.textContent = data.error;
        micError.style.display = 'flex';
        status.textContent = 'Ready';
        status.className = 'test-status';
        return;
    }

    micError.style.display = 'none';

    switch (data.status) {
        case 'loading':
            status.textContent = 'Loading model...';
            status.className = 'test-status';
            break;
        case 'recording':
            status.textContent = 'Listening...';
            status.className = 'test-status recording';
            break;
        case 'transcribing':
            status.textContent = 'Transcribing...';
            status.className = 'test-status transcribing';
            break;
        case 'idle':
            status.textContent = 'Ready';
            status.className = 'test-status';
            break;
    }
});

api.onTranscriptionComplete((data) => {
    const result = document.getElementById('test-result');
    if (data && data.text) {
        result.textContent = data.text;
    }
    setTimeout(() => {
        const status = document.getElementById('test-status');
        status.textContent = 'Ready';
        status.className = 'test-status';
    }, 500);
});

api.onSttShortcutChanged((data) => {
    const label = document.getElementById('test-shortcut-label');
    if (label && data && data.label) {
        label.textContent = data.label;
    }
});

document.addEventListener('DOMContentLoaded', async () => {
    try {
        const settings = await api.getSettings();
        const shortcutEl = document.getElementById('onboarding-shortcut');
        shortcutEl.value = settings.shortcuts.stt || 'Alt+Space';
        shortcutEl.dataset.previousValue = shortcutEl.value;
    } catch (e) {
        console.error('Failed to load settings:', e);
    }

    document.getElementById('btn-start').addEventListener('click', () => goToStep(2));

    document.getElementById('btn-model-back').addEventListener('click', () => goToStep(1));
    document.getElementById('btn-model-next').addEventListener('click', () => goToStep(3));

    document.getElementById('btn-access-back').addEventListener('click', () => goToStep(2));
    document.getElementById('btn-access-next').addEventListener('click', () => goToStep(4));
    document.getElementById('btn-access-skip').addEventListener('click', () => goToStep(4));
    document.getElementById('btn-grant-accessibility').addEventListener('click', () => {
        api.requestAccessibilityPermission();
    });

    document.getElementById('onboarding-shortcut').addEventListener('change', async (e) => {
        const select = e.target;
        const newShortcut = select.value;
        const previousValue = select.dataset.previousValue || 'Alt+Space';
        try {
            await api.updateSttShortcut(newShortcut);
            select.dataset.previousValue = newShortcut;
        } catch (err) {
            console.error('Failed to update shortcut:', err);
            select.value = previousValue;
        }
    });

    document.getElementById('btn-test-back').addEventListener('click', () => goToStep(3));
    document.getElementById('btn-finish').addEventListener('click', async () => {
        await api.completeOnboarding();
        try {
            const label = await api.getSttShortcutLabel();
            const { sendNotification } = window.__TAURI__.notification;
            sendNotification({
                title: 'Talk to Me is ready!',
                body: `Use ${label} to dictate.`,
            });
        } catch (e) {
            console.error('Failed to send notification:', e);
        }
        const { getCurrentWindow } = window.__TAURI__.window;
        await getCurrentWindow().close();
    });

    document.getElementById('btn-open-mic-prefs').addEventListener('click', () => {
        window.__TAURI__.opener.openUrl(
            'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
        );
    });
});
