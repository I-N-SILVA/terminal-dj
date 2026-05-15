use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ScUser {
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ScTrack {
    pub id:          u64,
    pub title:       String,
    pub user:        ScUser,
    pub duration:    u64,   // ms
    pub stream_url:  Option<String>,
    pub artwork_url: Option<String>,
}

impl ScTrack {
    pub fn display_artist(&self) -> &str { &self.user.username }
    pub fn duration_label(&self) -> String {
        let s = self.duration / 1000;
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub struct SoundCloudClient {
    pub(crate) client_id: String,
    pub(crate) http:      reqwest::Client,
}

impl SoundCloudClient {
    pub fn with_parts(client_id: String, http: reqwest::Client) -> Self {
        Self { client_id, http }
    }

    pub fn from_env() -> Option<Self> {
        let id = std::env::var("SOUNDCLOUD_CLIENT_ID").ok()?;
        if id.is_empty() { return None; }
        Some(Self {
            client_id: id,
            http: reqwest::Client::builder()
                .user_agent("terminal-dj/0.1")
                .build()
                .ok()?,
        })
    }

    pub async fn search(&self, query: &str) -> Result<Vec<ScTrack>> {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
        let enc = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        let url = format!(
            "https://api.soundcloud.com/tracks?q={}&client_id={}&limit=20",
            enc, self.client_id,
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            bail!("SoundCloud API: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;

        // v1 returns an array directly; v2 wraps in { collection: [...] }
        let arr = if let Some(col) = json.get("collection").and_then(|v| v.as_array()) {
            col.clone()
        } else if let Some(arr) = json.as_array() {
            arr.clone()
        } else {
            return Ok(vec![]);
        };

        Ok(arr.iter()
            .filter_map(|v| serde_json::from_value::<ScTrack>(v.clone()).ok())
            .filter(|t| t.stream_url.is_some())
            .collect())
    }

    /// Download the audio bytes for a track (blocks until complete).
    pub async fn get_stream_bytes(&self, track: &ScTrack) -> Result<Vec<u8>> {
        let base = track.stream_url.as_deref()
            .ok_or_else(|| anyhow::anyhow!("no stream_url"))?;
        let url = format!("{}?client_id={}", base, self.client_id);
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            bail!("stream failed: {}", resp.status());
        }
        Ok(resp.bytes().await?.to_vec())
    }
}
