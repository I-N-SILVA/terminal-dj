use anyhow::{bail, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Downloads audio from `url` via yt-dlp into `output_dir` as best-quality MP3.
/// Updates `status` with progress messages.
pub async fn download(url: &str, output_dir: &Path, status: Arc<Mutex<String>>) -> Result<()> {
    // Verify yt-dlp is available.
    let check = tokio::process::Command::new("yt-dlp").arg("--version").output().await;
    if check.is_err() || !check.unwrap().status.success() {
        bail!("yt-dlp not found — install it with: brew install yt-dlp");
    }

    *status.lock().await = format!("Downloading: {}", url);

    let template = output_dir
        .join("%(artist)s - %(title)s.%(ext)s")
        .to_string_lossy()
        .into_owned();

    let out = tokio::process::Command::new("yt-dlp")
        .args([
            "-x",
            "--audio-format",  "mp3",
            "--audio-quality", "0",
            "--embed-metadata",
            "--embed-thumbnail",
            "--no-playlist",
            "-o",              &template,
            url,
        ])
        .output()
        .await?;

    if out.status.success() {
        *status.lock().await = "Download complete — rescanning library...".to_string();
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let short = err.lines().last().unwrap_or("unknown error").to_string();
        bail!("yt-dlp: {}", short)
    }
}

/// Returns true if yt-dlp is installed.
#[allow(dead_code)]
pub async fn is_available() -> bool {
    tokio::process::Command::new("yt-dlp")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
