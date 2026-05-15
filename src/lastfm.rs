use anyhow::{anyhow, Result};
use reqwest::Client;
use std::collections::BTreeMap;

pub struct LastfmClient {
    api_key: String,
    api_secret: String,
    pub session_key: Option<String>,
    http: Client,
}

impl LastfmClient {
    pub fn from_env() -> Option<Self> {
        let api_key    = std::env::var("LASTFM_API_KEY").ok()?;
        let api_secret = std::env::var("LASTFM_API_SECRET").ok()?;
        Some(Self { api_key, api_secret, session_key: None, http: Client::new() })
    }

    fn sign(&self, params: &BTreeMap<&str, String>) -> String {
        let mut base = String::new();
        for (k, v) in params {
            if *k != "format" {
                base.push_str(k);
                base.push_str(v);
            }
        }
        base.push_str(&self.api_secret);
        format!("{:x}", md5::compute(base.as_bytes()))
    }

    pub async fn authenticate(&mut self) -> Result<()> {
        let username = std::env::var("LASTFM_USERNAME")?;
        let password = std::env::var("LASTFM_PASSWORD")?;
        let pw_md5   = format!("{:x}", md5::compute(password.as_bytes()));
        let auth_token = format!(
            "{:x}",
            md5::compute(format!("{}{}", username, pw_md5).as_bytes())
        );

        let mut params: BTreeMap<&str, String> = BTreeMap::new();
        params.insert("method",    "auth.getMobileSession".to_string());
        params.insert("api_key",   self.api_key.clone());
        params.insert("username",  username.clone());
        params.insert("authToken", auth_token);
        let sig = self.sign(&params);
        params.insert("api_sig",   sig);
        params.insert("format",    "json".to_string());

        let form: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp: serde_json::Value = self.http
            .post("https://ws.audioscrobbler.com/2.0/")
            .form(&form)
            .send().await?.json().await?;

        self.session_key = resp
            .get("session")
            .and_then(|s| s.get("key"))
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());

        if self.session_key.is_none() {
            return Err(anyhow!("Last.fm auth failed: {}", resp));
        }
        Ok(())
    }

    pub async fn now_playing(&self, artist: &str, title: &str) -> Result<()> {
        let sk = self.session_key.as_deref().ok_or_else(|| anyhow!("Not authenticated"))?;
        let mut params: BTreeMap<&str, String> = BTreeMap::new();
        params.insert("method",  "track.updateNowPlaying".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk",      sk.to_string());
        params.insert("artist",  artist.to_string());
        params.insert("track",   title.to_string());
        let sig = self.sign(&params);
        params.insert("api_sig", sig);
        params.insert("format",  "json".to_string());
        let form: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http.post("https://ws.audioscrobbler.com/2.0/").form(&form).send().await?;
        Ok(())
    }

    pub async fn scrobble(&self, artist: &str, title: &str, timestamp: u64) -> Result<()> {
        let sk = self.session_key.as_deref().ok_or_else(|| anyhow!("Not authenticated"))?;
        let ts = timestamp.to_string();
        let mut params: BTreeMap<&str, String> = BTreeMap::new();
        params.insert("method",       "track.scrobble".to_string());
        params.insert("api_key",      self.api_key.clone());
        params.insert("sk",           sk.to_string());
        params.insert("artist[0]",    artist.to_string());
        params.insert("track[0]",     title.to_string());
        params.insert("timestamp[0]", ts);
        let sig = self.sign(&params);
        params.insert("api_sig",  sig);
        params.insert("format",   "json".to_string());
        let form: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http.post("https://ws.audioscrobbler.com/2.0/").form(&form).send().await?;
        Ok(())
    }
}
