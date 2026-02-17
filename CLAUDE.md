# CLAUDE.md — Talk to Me

## What is this project?

A macOS menu bar utility for local speech-to-text (V1) and future text-to-speech, built with **Tauri v2** (Rust backend + vanilla HTML/CSS/JS frontend). Models run locally via whisper.cpp and ONNX Runtime — no cloud API, no data leaves the machine.

Full spec: `.claude/plans/talk-to-me-spec.md`

## Quick start

```bash
# Prerequisites: Rust toolchain, Node.js
cargo tauri dev          # Dev mode with hot-reload
cargo tauri build        # Production build (.dmg)
```

Models are downloaded at runtime into `~/Library/Application Support/TalkToMe/models/`.

## Architecture overview

```
src-tauri/src/
├── lib.rs              # App entry: tray icon, menu, global shortcut setup
├── state.rs            # AppState: Mutex-wrapped engine slots + settings
├── engine/
│   ├── mod.rs          # Traits: Engine → SttEngine / TtsEngine + factory
│   ├── whisper_stt.rs  # whisper-rs bindings (GGML models)
│   ├── onnx_stt.rs     # ONNX Runtime: Parakeet CTC + TDT decoding
│   └── onnx_tts.rs     # TTS stub (future Phase 6)
├── audio/
│   ├── capture.rs      # Mic capture via cpal (PCM f32, mono)
│   ├── playback.rs     # Audio output stub (future TTS)
│   └── processing.rs   # Resampling (rubato), mel spectrogram, FFT
├── commands/           # Tauri IPC commands (frontend ↔ backend)
│   ├── stt.rs          # start_recording / stop_recording / get_status
│   ├── models.rs       # download_model / set_active_model / delete_model
│   ├── settings.rs     # get_settings / update_settings / accessibility
│   └── tts.rs          # TTS stubs returning "not implemented"
├── hub/
│   ├── api.rs          # HuggingFace REST API (download URLs)
│   ├── download.rs     # Streaming download with progress events
│   └── registry.rs     # Model catalog (registry.json) + installed manifest
├── hotkey/mod.rs       # Global shortcut dispatch (Alt+Space → STT toggle)
└── platform/
    ├── mod.rs          # TextInjector / TextSelector traits + cfg dispatch
    └── macos.rs        # CGEvent keystroke injection + clipboard fallback

src/                    # Frontend (vanilla JS, no framework)
├── index.html          # Settings window (model management, preferences)
├── overlay.html        # Floating recording/transcription overlay
├── scripts/
│   ├── app.js          # Settings page logic
│   ├── overlay.js      # Overlay UI logic (recording state, animations)
│   └── api.js          # Tauri invoke() wrappers
└── styles/
    ├── main.css        # Settings window styles (dark theme)
    └── overlay.css     # Overlay styles (blurred, floating)
```

## Key design decisions

### Dual-engine STT
Two inference runtimes coexist behind the `SttEngine` trait:
- **whisper-rs** (`whisper_cpp`): Whisper models in GGML format. Mature, CoreML/Metal accelerated.
- **ort** (`onnx`): ONNX Runtime for NeMo Parakeet models. Supports CTC and TDT architectures.

The factory in `engine/mod.rs` dispatches based on `EngineType`.

### Parakeet model variants (onnx_stt.rs)
This is the most complex part of the codebase:

- **CTC** (e.g. `parakeet-ctc-0.6b`): Single `model.onnx`, 80 mel bins, greedy argmax decoding. Straightforward.
- **TDT** (e.g. `parakeet-tdt-0.6b-v3`): Two ONNX sessions (`encoder-model.onnx` + `decoder_joint-model.onnx`), 128 mel bins, autoregressive transducer decoding with duration predictions.

**TDT decoding gotchas** — learned the hard way:
- `targets` and `target_length` inputs must be **int32** (not int64). The ONNX model spec requires this.
- `length` input to the encoder is **int64**.
- Decoder output size = `vocab_size + 5` (5 duration classes for steps 0-4). `vocab_size` includes the blank token.
- LSTM states are `[2, 1, 640]` shape. Dimensions are auto-detected from first decoder output.
- Encoder output `[1, D, T']` must be transposed to `[T', D]` for frame-by-frame decoding.

### Mel spectrogram (processing.rs)
Custom Rust implementation (no Python dependency). NeMo-compatible parameters:
- 16kHz sample rate, n_fft=512, hop=160, **win_length=400** (25ms window)
- Per-feature normalization (zero mean, unit variance)
- CTC: 80 mels, TDT: 128 mels

### Platform abstraction
macOS-specific code (CGEvent, Accessibility API) is behind `TextInjector`/`TextSelector` traits in `platform/`. Adding Windows/Linux = implement these traits in `platform/windows.rs` or `platform/linux.rs`. The rest of the code never imports OS-specific APIs directly.

### Bidirectional by design
The architecture supports TTS from day one: separate engine slots in `AppState`, `TtsEngine` trait defined, stubs in place. Phase 6 activates TTS without refactoring.

## Model registry

`src-tauri/resources/registry.json` — embedded catalog of known models. Each entry specifies:
- `engine`: `"whisper_cpp"` or `"onnx"` → determines which engine loads it
- `capability`: `"speech_to_text"` or `"text_to_speech"`
- `files[]`: filenames, HF repo overrides, sizes
- `preprocessing`: mel config (sample_rate, n_mels) for ONNX models

Installed models are tracked in `~/Library/Application Support/TalkToMe/models/installed.json`.

**To add a new model**: add an entry to `registry.json` with the correct engine, files, and preprocessing config. If it's a new ONNX architecture (not CTC or TDT), you'll need to add decoding logic in `onnx_stt.rs`.

## STT pipeline flow

```
Alt+Space → hotkey/mod.rs → start_recording
  → cpal captures mic audio (f32 PCM)
Alt+Space again → stop_recording
  → Resample to 16kHz if needed (rubato)
  → Compute mel spectrogram (processing.rs)
  → Run inference (whisper_stt.rs or onnx_stt.rs)
  → Inject text via CGEvent or clipboard (platform/macos.rs)
  → Emit Tauri events for overlay UI updates
```

## Conventions

- **Error handling**: `anyhow::Result` everywhere in backend. Tauri commands convert to `String` errors at the boundary.
- **Logging**: `tracing` crate with structured logging. Use `tracing::info!`, `tracing::error!`, etc.
- **State**: All shared state goes through `AppState` with `Mutex` wrappers. One STT engine + one TTS engine active at a time.
- **Frontend**: No framework. Vanilla JS with `window.__TAURI__.core.invoke()` for IPC. Dark macOS-native theme.
- **Naming**: Rust modules use snake_case. Model IDs follow HuggingFace format (`org/model-name`). Model directories on disk use `--` as separator (`org--model-name`).

## Build notes

- **ONNX Runtime**: The `ort` crate uses default features (includes `download-binaries` for static linking). Do NOT add `load-dynamic` — it causes runtime `dlopen` failures.
- **whisper-rs**: Statically links whisper.cpp. First build downloads and compiles it (~2-3 min).
- **macOS minimum**: 13.0 (Ventura). Set in `tauri.conf.json`.
- **Target**: macOS only for now. Cross-platform prepared via `platform/` abstraction.

## Development phases (from spec)

| Phase | Status | Description |
|-------|--------|-------------|
| 1. Foundations | **Done** | Whisper STT, tray icon, overlay, hotkey, text injection |
| 2. HuggingFace + Models | **Done** | Download, catalog, model management UI |
| 3. Parakeet / ONNX | **Done** | CTC + TDT decoding, mel spectrogram |
| 4. Polish | Pending | Waveform animation, sound feedback, onboarding, error UX |
| 5. Distribution | Pending | Code signing, CI/CD, .dmg builds |
| 6. TTS | Future | Piper/Bark voices, playback, text selection |
