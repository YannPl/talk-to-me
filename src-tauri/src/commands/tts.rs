#[tauri::command]
pub fn speak_selected_text() -> Result<(), String> {
    Err("TTS not yet implemented (coming in V2)".into())
}

#[tauri::command]
pub fn speak_text(_text: String) -> Result<(), String> {
    Err("TTS not yet implemented (coming in V2)".into())
}

#[tauri::command]
pub fn stop_speaking() -> Result<(), String> {
    Err("TTS not yet implemented (coming in V2)".into())
}
