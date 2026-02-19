# First-Launch Onboarding Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a first-launch wizard that guides users through model download, accessibility permission, shortcut selection, and a live transcription test.

**Architecture:** New `onboarding` Tauri window with dedicated HTML/JS/CSS. Backend adds `onboarding_completed` flag to settings, new permission-check commands, and conditional launch logic. Post-onboarding: permission status banner in Settings for returning users.

**Tech Stack:** Tauri v2, Rust (backend), vanilla HTML/CSS/JS (frontend)

---

### Task 1: Add `onboarding_completed` to GeneralSettings

**Files:**
- Modify: `src-tauri/src/state.rs:165-178`

**Step 1: Add the field**

In `GeneralSettings`, add `onboarding_completed` with `#[serde(default)]` so existing serialized settings deserialize without breaking:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub sound_feedback: bool,
    #[serde(default)]
    pub onboarding_completed: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            sound_feedback: true,
            onboarding_completed: false,
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p talk-to-me`
Expected: Compiles with no errors.

**Step 3: Update `saveSettings` in app.js to include the new field**

In `src/scripts/app.js`, the `saveSettings()` function constructs a `Settings` object. Add `onboarding_completed` to `general`:

```javascript
general: {
    launch_at_login: document.getElementById('launch-at-login').checked,
    sound_feedback: document.getElementById('sound-feedback').checked,
    onboarding_completed: true,  // preserve — already completed if in settings
},
```

**Step 4: Commit**

```bash
git add src-tauri/src/state.rs src/scripts/app.js
git commit -m "feat: add onboarding_completed flag to GeneralSettings"
```

---

### Task 2: Add new Tauri commands (complete_onboarding, check_microphone_permission)

**Files:**
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/lib.rs:29-49` (register new commands)

**Step 1: Add `complete_onboarding` command**

In `src-tauri/src/commands/settings.rs`, add:

```rust
#[tauri::command]
pub fn complete_onboarding(app_handle: AppHandle) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    state.settings.lock().unwrap().general.onboarding_completed = true;
    crate::persistence::save_settings(&app_handle);
    Ok(())
}
```

**Step 2: Add `check_microphone_permission` command**

This attempts a brief access to the default input device to check if microphone is available. It does NOT record — just checks if `cpal` can see an input device.

```rust
#[tauri::command]
pub fn check_microphone_permission() -> Result<bool, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    match host.default_input_device() {
        Some(device) => {
            match device.default_input_config() {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        None => Ok(false),
    }
}
```

**Step 3: Register the new commands in lib.rs**

Add to the `invoke_handler` in `src-tauri/src/lib.rs`:

```rust
commands::settings::complete_onboarding,
commands::settings::check_microphone_permission,
```

**Step 4: Verify it compiles**

Run: `cargo check -p talk-to-me`
Expected: Compiles with no errors.

**Step 5: Commit**

```bash
git add src-tauri/src/commands/settings.rs src-tauri/src/lib.rs
git commit -m "feat: add complete_onboarding and check_microphone_permission commands"
```

---

### Task 3: Add api.js wrappers for new commands

**Files:**
- Modify: `src/scripts/api.js`

**Step 1: Add the new wrappers**

At the end of the existing exports in `src/scripts/api.js`:

```javascript
export const completeOnboarding = () => invoke('complete_onboarding');
export const checkMicrophonePermission = () => invoke('check_microphone_permission');
```

**Step 2: Commit**

```bash
git add src/scripts/api.js
git commit -m "feat: add API wrappers for onboarding commands"
```

---

### Task 4: Add onboarding window to Tauri config and capabilities

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

**Step 1: Add the window definition in tauri.conf.json**

Add a third entry to `app.windows` array, after the `overlay` window:

```json
{
  "label": "onboarding",
  "url": "onboarding.html",
  "title": "Talk to Me",
  "width": 600,
  "height": 500,
  "resizable": false,
  "center": true,
  "decorations": true,
  "visible": false
}
```

**Step 2: Add the onboarding window to capabilities**

In `src-tauri/capabilities/default.json`, add `"onboarding"` to the `windows` array:

```json
"windows": ["main", "overlay", "onboarding"],
```

**Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/capabilities/default.json
git commit -m "feat: add onboarding window to Tauri config and capabilities"
```

---

### Task 5: Create onboarding HTML page

**Files:**
- Create: `src/onboarding.html`

**Step 1: Create the wizard HTML**

Create `src/onboarding.html` with a 4-step wizard structure. Steps are shown/hidden via CSS classes. Each step has its own section.

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Talk to Me</title>
    <link rel="stylesheet" href="styles/onboarding.css">
</head>
<body>
    <!-- Progress indicator -->
    <div class="wizard-progress">
        <div class="progress-step active" data-step="1">1</div>
        <div class="progress-line"></div>
        <div class="progress-step" data-step="2">2</div>
        <div class="progress-line"></div>
        <div class="progress-step" data-step="3">3</div>
        <div class="progress-line"></div>
        <div class="progress-step" data-step="4">4</div>
    </div>

    <!-- Step 1: Welcome -->
    <section class="wizard-step active" id="step-welcome">
        <div class="step-content">
            <h1>Welcome to Talk to Me</h1>
            <p class="subtitle">Local speech-to-text for macOS. Your voice stays on your Mac.</p>
            <p class="description">Let's set up a few things so you can start dictating right away.</p>
        </div>
        <div class="step-actions">
            <button class="btn-primary" id="btn-start">Get Started</button>
        </div>
    </section>

    <!-- Step 2: Model Download -->
    <section class="wizard-step" id="step-model">
        <div class="step-content">
            <h1>Choose a transcription model</h1>
            <p class="subtitle">Models run locally on your Mac. Pick one to get started.</p>
            <div id="onboarding-catalog" class="model-list"></div>
            <div id="onboarding-installed" class="installed-status"></div>
        </div>
        <div class="step-actions">
            <button class="btn-secondary" id="btn-model-back">Back</button>
            <button class="btn-primary" id="btn-model-next" disabled>Next</button>
        </div>
    </section>

    <!-- Step 3: Accessibility + Shortcut -->
    <section class="wizard-step" id="step-accessibility">
        <div class="step-content">
            <h1>Permissions & Shortcut</h1>

            <div class="setting-group">
                <h3>Dictation Shortcut</h3>
                <p class="setting-desc">Choose which key combination triggers dictation.</p>
                <select id="onboarding-shortcut">
                    <option value="Alt+Space">&#x2325; Space</option>
                    <option value="Ctrl+Space">&#x2303; Space</option>
                    <option value="Super+Shift+Space">&#x2318;&#x21E7; Space</option>
                    <option value="RightCommand">Right &#x2318;</option>
                </select>
            </div>

            <div class="setting-group">
                <h3>Accessibility Permission</h3>
                <p class="setting-desc">Talk to Me needs Accessibility permission to type the transcribed text into your apps.</p>
                <div class="permission-row">
                    <span id="accessibility-indicator" class="permission-indicator pending">Checking...</span>
                    <button class="btn-secondary" id="btn-grant-accessibility">Grant Permission</button>
                </div>
                <p class="permission-skip-hint" id="accessibility-skip-hint" style="display:none;">
                    Without this, text will be copied to your clipboard instead of typed automatically.
                </p>
            </div>
        </div>
        <div class="step-actions">
            <button class="btn-secondary" id="btn-access-back">Back</button>
            <button class="btn-primary" id="btn-access-next">Next</button>
            <button class="btn-text" id="btn-access-skip" style="display:none;">Skip for now</button>
        </div>
    </section>

    <!-- Step 4: Test -->
    <section class="wizard-step" id="step-test">
        <div class="step-content">
            <h1>Try it out!</h1>
            <p class="subtitle" id="test-instruction">Press <strong id="test-shortcut-label">&#x2325;Space</strong> to start dictating, then press again to stop.</p>
            <div id="test-area" class="test-area">
                <div id="test-status" class="test-status">Ready</div>
                <div id="test-result" class="test-result"></div>
            </div>
            <div id="mic-error" class="error-banner" style="display:none;">
                <span id="mic-error-text"></span>
                <button class="btn-secondary btn-small" id="btn-open-mic-prefs">Open System Preferences</button>
            </div>
        </div>
        <div class="step-actions">
            <button class="btn-secondary" id="btn-test-back">Back</button>
            <button class="btn-primary" id="btn-finish">Finish</button>
        </div>
    </section>

    <script type="module" src="scripts/onboarding.js"></script>
</body>
</html>
```

**Step 2: Commit**

```bash
git add src/onboarding.html
git commit -m "feat: create onboarding wizard HTML page"
```

---

### Task 6: Create onboarding CSS

**Files:**
- Create: `src/styles/onboarding.css`

**Step 1: Create the stylesheet**

Reuse the same CSS variables and design tokens from `main.css` (copy the `:root` block). Add wizard-specific styles: progress indicators, step transitions, test area, permission indicators, model cards for the catalog.

Key styles to include:
- `:root` variables (same as main.css)
- `body` and base styles (same dark theme)
- `.wizard-progress` — horizontal progress steps at the top
- `.wizard-step` — hidden by default, `.active` shows
- `.step-content` — centered, padded content area
- `.step-actions` — bottom button row
- `.btn-primary` — blue filled button
- `.btn-secondary` — ghost button
- `.btn-text` — text-only button for "skip"
- `.model-list` — reuse model card styles from main.css
- `.permission-indicator` — status with color states (.granted, .pending, .denied)
- `.test-area` — bordered area for test results
- `.test-status` — recording status indicator
- `.test-result` — transcription output
- `.error-banner` — orange warning with action button
- `.badge-recommended` — blue badge for recommended model
- Progress bar and download styles (reuse from main.css)

Note: The full CSS should be a cohesive dark theme matching the existing Settings window aesthetic. It should be roughly 300-400 lines.

**Step 2: Commit**

```bash
git add src/styles/onboarding.css
git commit -m "feat: create onboarding wizard CSS"
```

---

### Task 7: Create onboarding JavaScript

**Files:**
- Create: `src/scripts/onboarding.js`

**Step 1: Create the wizard logic**

This is the most complex file. It handles:

1. **Step navigation** — Show/hide steps, update progress indicator
2. **Step 2 (Model)** — Load catalog, download model, show progress, auto-activate
3. **Step 3 (Accessibility + Shortcut)** — Poll `AXIsProcessTrusted` every 2s, shortcut dropdown updates the registered shortcut
4. **Step 4 (Test)** — Listen for recording/transcription events, display results, handle mic errors
5. **Finish** — Call `complete_onboarding`, close onboarding window

Key implementation details:

```javascript
import * as api from './api.js';

const RECOMMENDED_MODEL = 'openai/whisper-large-v3-turbo';
let currentStep = 1;
let accessibilityPollingId = null;
let installedModelId = null;

// --- Step navigation ---
function goToStep(step) { /* show/hide steps, update progress */ }

// --- Step 2: Model catalog ---
// Reuse api.getCatalog('stt') and api.downloadModel()
// Listen to download-progress, download-complete, download-error events
// After download: call api.setActiveModel(modelId, 'stt')
// Enable "Next" button once a model is installed and active

// --- Step 3: Accessibility ---
// On enter: start polling checkAccessibilityPermission every 2s
// "Grant Permission" button calls requestAccessibilityPermission
// Update indicator: green check when granted
// Shortcut dropdown: on change, call api.updateSttShortcut(value)
// On leave: stop polling

// --- Step 4: Test ---
// Listen to 'recording-status', 'transcription-complete' events
// Update test-status based on recording state
// Show transcribed text in test-result
// If recording fails (no mic): show error banner with link to System Preferences

// --- Finish ---
// Call api.completeOnboarding()
// Close onboarding window: window.__TAURI__.window.getCurrentWindow().close()
```

Event listeners from `api.js` to use:
- `onDownloadProgress`, `onDownloadComplete`, `onDownloadError` — model download
- `onRecordingStatus` — recording state changes
- `onTranscriptionComplete` — transcription result
- `onSttShortcutChanged` — update test-shortcut-label

For the shortcut change in the onboarding: call `api.updateSttShortcut(newValue)`. This handles unregister old + register new + persist. Update the test step's label accordingly.

For accessibility polling:
```javascript
function startAccessibilityPolling() {
    checkAccessibilityNow();
    accessibilityPollingId = setInterval(checkAccessibilityNow, 2000);
}
function stopAccessibilityPolling() {
    if (accessibilityPollingId) clearInterval(accessibilityPollingId);
}
async function checkAccessibilityNow() {
    const granted = await api.checkAccessibilityPermission();
    // update UI indicator
}
```

For the "Open System Preferences > Microphone" button:
```javascript
window.__TAURI__.opener.openUrl(
    'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
);
```

**Step 2: Commit**

```bash
git add src/scripts/onboarding.js
git commit -m "feat: create onboarding wizard JavaScript logic"
```

---

### Task 8: Backend launch logic — show onboarding or normal startup

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add conditional window display at end of setup**

After the existing setup code (line ~274, before `Ok(())`), add logic to check `onboarding_completed` and show the appropriate window:

```rust
// Show onboarding wizard on first launch, otherwise check permissions
{
    let state = app.state::<AppState>();
    let onboarding_completed = state.settings.lock().unwrap().general.onboarding_completed;

    if !onboarding_completed {
        if let Some(window) = app.get_webview_window("onboarding") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    } else {
        // Check permissions and emit event if any are missing
        let accessibility_ok = {
            let injector = platform::get_text_injector();
            injector.is_accessibility_granted()
        };
        if !accessibility_ok {
            let _ = app.emit("permission-missing", serde_json::json!({
                "permission": "accessibility"
            }));
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p talk-to-me`

**Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: show onboarding wizard on first launch, check permissions on subsequent launches"
```

---

### Task 9: Add permission banner to Settings (for returning users)

**Files:**
- Modify: `src/index.html`
- Modify: `src/scripts/app.js`
- Modify: `src/styles/main.css`

**Step 1: Add a permission banner HTML element in index.html**

At the top of `.settings-container`, before the tab bar:

```html
<div id="permission-banner" class="permission-banner" style="display:none;">
    <span id="permission-banner-text"></span>
    <button id="permission-banner-btn" class="btn-secondary btn-small">Fix</button>
    <button id="permission-banner-dismiss" class="btn-dismiss">&times;</button>
</div>
```

**Step 2: Add banner styles in main.css**

```css
.permission-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: rgba(255, 159, 10, 0.15);
    border-bottom: 1px solid rgba(255, 159, 10, 0.3);
    font-size: 12px;
    color: var(--accent-orange);
    flex-shrink: 0;
}

.permission-banner .btn-small {
    font-size: 11px;
    padding: 3px 8px;
}

.btn-dismiss {
    font-family: var(--font-stack);
    font-size: 14px;
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    margin-left: auto;
    padding: 2px 6px;
}

.btn-dismiss:hover {
    color: var(--text-primary);
}
```

**Step 3: Add banner logic in app.js**

Listen for the `permission-missing` event emitted by the backend:

```javascript
api.onPermissionMissing((data) => {
    const banner = document.getElementById('permission-banner');
    const text = document.getElementById('permission-banner-text');
    const btn = document.getElementById('permission-banner-btn');

    if (data.permission === 'accessibility') {
        text.textContent = 'Accessibility permission is disabled. Text will be copied to clipboard instead of typed.';
        btn.textContent = 'Open Preferences';
        btn.onclick = () => api.requestAccessibilityPermission();
    }
    banner.style.display = 'flex';
});

document.getElementById('permission-banner-dismiss')?.addEventListener('click', () => {
    document.getElementById('permission-banner').style.display = 'none';
});
```

Also add the `onPermissionMissing` listener wrapper to `api.js`:

```javascript
export const onPermissionMissing = (callback) => listen('permission-missing', (e) => callback(e.payload));
```

**Step 4: Commit**

```bash
git add src/index.html src/scripts/app.js src/styles/main.css src/scripts/api.js
git commit -m "feat: add permission status banner in Settings for returning users"
```

---

### Task 10: Handle onboarding window close event in backend

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add close handler for onboarding window**

Similar to the main window close handler. When onboarding window is closed, just hide it (don't prevent close — let it close naturally when user clicks Finish). But if user force-closes it, the app should remain in the tray.

Add after the existing main window close handler (around line 264-272):

```rust
if let Some(window) = app.get_webview_window("onboarding") {
    let w = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = w.hide();
        }
    });
}
```

**Step 2: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: handle onboarding window close event"
```

---

### Task 11: Integration test — run and verify

**Step 1: Reset onboarding flag for testing**

Temporarily, you can delete the settings store to simulate a first launch:
```bash
rm ~/Library/Application\ Support/com.yannpl.ttm/settings.json
```

**Step 2: Run the app**

```bash
cargo tauri dev
```

**Step 3: Verify the wizard flow**

1. On launch, the onboarding wizard window should appear
2. Step 1 (Welcome): "Get Started" button advances to Step 2
3. Step 2 (Model): Catalog loads, Whisper Large v3 Turbo shown as recommended. Download a model. "Next" becomes enabled after download.
4. Step 3 (Accessibility + Shortcut): Shortcut dropdown works. Accessibility status shows correctly. "Grant Permission" opens System Preferences. Polling detects when granted.
5. Step 4 (Test): Pressing the shortcut triggers recording. Microphone permission dialog appears (first time). Transcription result shows in the test area.
6. "Finish" completes onboarding, closes wizard, app is in menu bar.
7. Restart: No wizard. If accessibility was revoked, banner shows in Settings.

**Step 4: Commit any fixes found during testing**

---

### Task 12: Update GitHub issue status

**Step 1: Update issue #7 on GitHub**

```bash
gh issue edit 7 --add-label "in-progress"
```

Or if the implementation is complete and verified:

```bash
gh issue close 7 --comment "Implemented first-launch onboarding wizard with:
- 4-step wizard (Welcome, Model Download, Accessibility+Shortcut, Live Test)
- Permission checking on subsequent launches with banner in Settings
- Microphone permission triggered via live transcription test
- Configurable shortcut in onboarding flow"
```

**Step 2: Move the issue in the project board if applicable**

```bash
# Check current project status
gh issue view 7 --json projectItems
```
