# First-Launch Onboarding Design

**Date:** 2026-02-19
**Issue:** [#7 — First-launch onboarding (permissions + model download)](https://github.com/YannPl/talk-to-me/issues/7)

## Overview

Guide new users through permissions and model setup on first launch via a dedicated wizard window. Also detect revoked permissions on subsequent launches and notify the user.

## Wizard Flow (4 steps)

### Step 1 — Welcome
- Title: "Bienvenue dans Talk to Me"
- Short description: "Dictée vocale locale — vos données restent sur votre Mac."
- App icon
- Button: "Commencer"

### Step 2 — Model Download
- Title: "Choisissez un modèle de transcription"
- Full model catalog from `registry.json`
- `openai/whisper-large-v3-turbo` ("Whisper Large v3 Turbo") pre-selected with "Recommandé" badge
- Download progress bar
- Model automatically activated once downloaded
- "Suivant" button enabled only when a model is installed and loaded

### Step 3 — Accessibility + Shortcut
- **Shortcut section:** Dropdown matching Settings (Alt+Space, Ctrl+Space, RightCommand, etc.)
- **Accessibility section:**
  - Explanation: "Talk to Me a besoin de l'autorisation Accessibilité pour injecter le texte dicté dans vos applications."
  - "Autoriser l'Accessibilité" button → opens System Preferences
  - Status indicator: green check if granted, orange warning if not
  - Poll `AXIsProcessTrusted()` every 2s to detect when granted
  - "Suivant" enabled when Accessibility is granted
  - "Skip" option (discreet) with warning: "Sans cette permission, le texte sera copié dans le presse-papiers au lieu d'être tapé automatiquement."

### Step 4 — Microphone Test
- Title: "Testez votre premier enregistrement"
- Instruction: "Appuyez sur **[chosen shortcut]** pour commencer à dicter, puis appuyez à nouveau pour arrêter."
- Transcribed text display area
- Microphone permission: macOS shows native dialog on first recording attempt
- If mic denied: "Permission microphone nécessaire" + "Ouvrir Préférences Système > Microphone" button
- "Terminer" button when test succeeds
- Final notification: "Talk to Me est prêt ! Utilisez **[shortcut]** pour dicter."
- App minimizes to menu bar

## Architecture

### New Files
- `src/onboarding.html` — Wizard page
- `src/scripts/onboarding.js` — Step logic, permission polling, transcription test
- `src/styles/onboarding.css` — Wizard styles (dark theme, consistent with app)

### Modified Files
- `src-tauri/tauri.conf.json` — Add `onboarding` window definition
- `src-tauri/src/lib.rs` — Launch logic: check `onboarding_completed`, show wizard or normal startup
- `src-tauri/src/state.rs` — Add `onboarding_completed: bool` to `GeneralSettings`
- `src-tauri/src/commands/settings.rs` — New commands: `check_microphone_permission()`, `complete_onboarding()`
- `src/scripts/app.js` — Permission status section in Settings with "Vérifier les permissions" button

### Tauri Window Config
```json
{
  "label": "onboarding",
  "url": "onboarding.html",
  "title": "Talk to Me — Configuration",
  "width": 600,
  "height": 500,
  "resizable": false,
  "center": true,
  "decorations": true,
  "visible": false
}
```

### Launch Logic (lib.rs)
```
At setup:
1. Load settings
2. If onboarding_completed == false:
   → Show onboarding window (hide others)
   → Wait for onboarding completion (Tauri event)
3. If onboarding_completed == true:
   → Check permissions (Accessibility + Mic)
   → If permission missing: system notification + banner in Settings
   → Normal startup (tray icon, hotkey, etc.)
```

### Microphone Permission Check
New Tauri command `check_microphone_permission`:
- Attempts brief access to default input device via cpal (without recording)
- Returns `true`/`false` based on device accessibility

### Post-Onboarding Permission Detection
- On every launch, `lib.rs` checks Accessibility via `AXIsProcessTrusted()`
- If revoked: emit Tauri event `permission-missing` with permission type
- In Settings: orange alert banner "⚠ Permission Accessibilité désactivée — [Réactiver]"
- Button opens relevant System Preferences pane

## Error Handling

### During Onboarding

| Situation | Behavior |
|---|---|
| Model download fails (network) | Error message + "Réessayer" button. Step stays blocked. |
| Accessibility refused | Explanation message + button to reopen System Preferences. Step blocked (skip available). |
| Mic refused (native dialog) | "Permission microphone nécessaire" + "Ouvrir Préférences Système > Microphone" button. |
| No audio device | "Aucun microphone détecté. Branchez un micro et réessayez." |
| User closes wizard | App stays in menu bar. Wizard reappears on next launch (flag still `false`). |
| Transcription test fails | Error message + suggest re-downloading model or choosing another. |

### Post-Onboarding (Subsequent Launches)

| Situation | Behavior |
|---|---|
| Accessibility revoked | On launch: system notification. In Settings: orange banner with "Réactiver" button. |
| Mic revoked | Detected at recording time. Overlay message: "Permission microphone désactivée" + System Preferences link. |
| Shortcut conflict | Fallback to Alt+Space (existing behavior) + notification in Settings. |

## State Changes

Add to `GeneralSettings`:
```rust
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub sound_feedback: bool,
    pub onboarding_completed: bool,  // NEW — default: false
}
```

## New Tauri Commands

- `check_microphone_permission() -> bool` — Check if mic access is available
- `complete_onboarding()` — Set `onboarding_completed = true` and persist
- `request_microphone_permission()` — Open System Preferences > Microphone (fallback when native dialog was already dismissed)
