use anyhow::{bail, Result};
use std::path::Path;

/// Helper to find either yt-dlp or youtube-dl executable name.
async fn find_ytdl_executable() -> Option<&'static str> {
    // Try yt-dlp
    if let Ok(out) = tokio::process::Command::new("yt-dlp")
        .arg("--version")
        .output()
        .await
    {
        if out.status.success() {
            return Some("yt-dlp");
        }
    }
    // Try youtube-dl
    if let Ok(out) = tokio::process::Command::new("youtube-dl")
        .arg("--version")
        .output()
        .await
    {
        if out.status.success() {
            return Some("youtube-dl");
        }
    }
    None
}

pub async fn download_with_progress(
    url: &str,
    output_dir: &Path,
    state: std::sync::Arc<std::sync::Mutex<crate::app::DownloadState>>,
) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncBufReadExt;

    let executable = match find_ytdl_executable().await {
        Some(exe) => exe,
        None => bail!("Neither yt-dlp nor youtube-dl found"),
    };

    let template = output_dir
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .into_owned();

    let mut cmd = tokio::process::Command::new(executable);
    cmd.args([
        "-x",
        "--audio-format",
        "mp3",
        "--audio-quality",
        "0",
        "--embed-metadata",
        "--embed-thumbnail",
        "--no-playlist",
        "--newline",
        "-o",
        &template,
        url,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("Failed to grab stdout");

    let state_clone = state.clone();

    let mut reader = tokio::io::BufReader::new(stdout).lines();
    tokio::spawn(async move {
        while let Ok(Some(line)) = reader.next_line().await {
            let mut st = state_clone.lock().unwrap();
            if line.contains("[download]") {
                if line.contains("Destination: ") {
                    let dest = line.split("Destination: ").last().unwrap_or("").trim();
                    // Just store it, but wait for ExtractAudio for final mp3 if possible
                    st.filename = Some(std::path::PathBuf::from(dest));
                    st.message = "Downloading...".to_string();
                } else if line.contains("%") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for p in parts {
                        if p.ends_with("%") {
                            if let Ok(num) = p.trim_end_matches('%').parse::<f64>() {
                                st.progress = num;
                            }
                        }
                    }
                    st.message = line.replace("[download]", "").trim().to_string();
                }
            } else if line.contains("[ExtractAudio]") && line.contains("Destination: ") {
                let dest = line.split("Destination: ").last().unwrap_or("").trim();
                st.filename = Some(std::path::PathBuf::from(dest));
                st.message = "Extracting Audio...".to_string();
            } else if line.contains("[ExtractAudio]") {
                st.message = line.replace("[ExtractAudio]", "").trim().to_string();
                st.progress = 100.0;
            }
        }
    });

    let status = child.wait().await?;

    let mut final_state = state.lock().unwrap();
    final_state.is_downloading = false;
    if status.success() {
        final_state.progress = 100.0;
        final_state.message = "Download Complete".to_string();
    } else {
        final_state.message = "Download Failed".to_string();
    }

    Ok(())
}

/// Returns true if either yt-dlp or youtube-dl is installed.
#[allow(dead_code)]
pub async fn is_available() -> bool {
    find_ytdl_executable().await.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_downloader_detection() {
        let exists = is_available().await;
        println!("yt-dlp / youtube-dl available: {}", exists);
    }
}
