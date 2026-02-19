const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

export const listInstalledModels = (capability) =>
    invoke('list_installed_models', { capability });

export const getCatalog = (capability) =>
    invoke('get_catalog', { capability });

export const downloadModel = (modelId) =>
    invoke('download_model', { modelId });

export const deleteModel = (modelId) =>
    invoke('delete_model', { modelId });

export const cancelDownload = (modelId) =>
    invoke('cancel_download', { modelId });

export const setActiveModel = (modelId, capability) =>
    invoke('set_active_model', { modelId, capability });

export const getActiveModel = (capability) =>
    invoke('get_active_model', { capability });

export const startRecording = () => invoke('start_recording');
export const stopRecording = () => invoke('stop_recording');
export const getStatus = () => invoke('get_status');

export const speakSelectedText = () => invoke('speak_selected_text');
export const speakText = (text) => invoke('speak_text', { text });
export const stopSpeaking = () => invoke('stop_speaking');

export const getSettings = () => invoke('get_settings');
export const updateSettings = (settings) => invoke('update_settings', { settings });
export const updateSttShortcut = (shortcut) => invoke('update_stt_shortcut', { shortcut });
export const checkAccessibilityPermission = () => invoke('check_accessibility_permission');
export const requestAccessibilityPermission = () => invoke('request_accessibility_permission');
export const getSttShortcutLabel = () => invoke('get_stt_shortcut_label');
export const getAppVersion = () => invoke('get_app_version');

export const onDownloadProgress = (callback) => listen('download-progress', (e) => callback(e.payload));
export const onDownloadComplete = (callback) => listen('download-complete', (e) => callback(e.payload));
export const onDownloadError = (callback) => listen('download-error', (e) => callback(e.payload));
export const onRecordingStatus = (callback) => listen('recording-status', (e) => callback(e.payload));
export const onAudioLevel = (callback) => listen('audio-level', (e) => callback(e.payload));
export const onTranscriptionComplete = (callback) => listen('transcription-complete', (e) => callback(e.payload));
export const onTranscriptionProgress = (callback) => listen('transcription-progress', (e) => callback(e.payload));
export const onStreamingTranscription = (callback) => listen('streaming-transcription', (e) => callback(e.payload));
export const onOverlayMode = (callback) => listen('overlay-mode', (e) => callback(e.payload));
export const onPlaybackStatus = (callback) => listen('playback-status', (e) => callback(e.payload));
export const onPlaybackProgress = (callback) => listen('playback-progress', (e) => callback(e.payload));
export const onSttShortcutChanged = (callback) => listen('stt-shortcut-changed', (e) => callback(e.payload));
export const onNavigateTab = (callback) => listen('navigate-tab', (e) => callback(e.payload));
