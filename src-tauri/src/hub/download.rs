use std::path::Path;
use std::sync::atomic::Ordering;
use anyhow::{Result, Context};
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use serde::Serialize;

use crate::state::CancelFlag;

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub progress: f64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
}

pub async fn download_file(
    app_handle: &AppHandle,
    model_id: &str,
    url: &str,
    dest: &Path,
    expected_size: u64,
    cancel_flag: &CancelFlag,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create model directory")?;
    }

    let client = reqwest::Client::new();

    let head_resp = client.head(url)
        .header("User-Agent", "TalkToMe/0.1")
        .send().await?
        .error_for_status()?;

    let total_size = head_resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(expected_size);

    let mut downloaded: u64 = 0;
    let mut request = client.get(url)
        .header("User-Agent", "TalkToMe/0.1");

    if dest.exists() {
        let file_len = std::fs::metadata(dest)?.len();
        if file_len >= total_size && total_size > 0 {
            tracing::info!("File already fully downloaded ({} bytes)", file_len);
            return Ok(());
        } else if file_len > 0 {
            downloaded = file_len;
            request = request.header("Range", format!("bytes={}-", downloaded));
            tracing::info!("Resuming download from {} / {} bytes", downloaded, total_size);
        }
    }

    let response = request.send().await?.error_for_status()?;

    let mut file = if downloaded > 0 {
        std::fs::OpenOptions::new().append(true).open(dest)?
    } else {
        std::fs::File::create(dest)?
    };

    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("Download cancelled: {}", model_id);
            anyhow::bail!("cancelled");
        }

        let chunk = chunk.context("Error reading download stream")?;
        std::io::Write::write_all(&mut file, &chunk)?;
        downloaded += chunk.len() as u64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 { (downloaded as f64 / elapsed) as u64 } else { 0 };
        let remaining = if speed > 0 && total_size > downloaded {
            (total_size - downloaded) / speed
        } else {
            0
        };

        let _ = app_handle.emit("download-progress", DownloadProgress {
            model_id: model_id.to_string(),
            progress: if total_size > 0 { downloaded as f64 / total_size as f64 } else { 0.0 },
            speed_bps: speed,
            eta_seconds: remaining,
        });
    }

    tracing::info!("Download complete: {} ({} bytes)", dest.display(), downloaded);
    Ok(())
}
