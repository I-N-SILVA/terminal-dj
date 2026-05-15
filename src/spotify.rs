use anyhow::{Result, anyhow};
use rspotify::{
    clients::{BaseClient, OAuthClient},
    scopes, AuthCodePkceSpotify, Credentials, OAuth,
    Token,
};
use tracing;
use std::fs;
use std::path::PathBuf;

/// Minimal playlist info — just what the app actually uses.
/// Using our own type rather than rspotify's `SimplifiedPlaylist` lets us
/// deserialize the raw JSON ourselves and tolerate missing optional fields
/// (e.g. `tracks`) that rspotify 0.13 incorrectly treats as required.
#[derive(Debug, Clone)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
}

pub struct SpotifyClient {
    pub spotify: AuthCodePkceSpotify,
    token_path: PathBuf,
}

impl SpotifyClient {
    pub fn new() -> Self {
        let mut token_path = dirs::config_dir().unwrap_or_default();
        token_path.push("terminal-dj");
        token_path.push("spotify_token.json");

        let creds_env = Credentials::from_env();
        let creds = creds_env.unwrap_or_else(|| Credentials::new("ENTER_CLIENT_ID_HERE", ""));
        
        if creds.id == "ENTER_CLIENT_ID_HERE" {
            // We could log a warning or set a flag in App to show a message
        }
        let oauth = OAuth {
            redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
            scopes: scopes!(
                "user-read-playback-state",
                "user-modify-playback-state",
                "user-read-currently-playing",
                "playlist-read-private",
                "playlist-read-collaborative",
                "user-library-read"
            ),
            ..Default::default()
        };

        let spotify = AuthCodePkceSpotify::new(creds, oauth);
        SpotifyClient { spotify, token_path }
    }

    pub async fn load_token(&self) -> Result<bool> {
        if self.token_path.exists() {
            let json = fs::read_to_string(&self.token_path)?;
            let token: Token = serde_json::from_str(&json)?;
            *self.spotify.get_token().lock().await.unwrap() = Some(token);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn save_token(&self) -> Result<()> {
        if let Some(token) = &*self.spotify.get_token().lock().await.unwrap() {
            let json = serde_json::to_string_pretty(token)?;
            fs::create_dir_all(self.token_path.parent().unwrap())?;
            fs::write(&self.token_path, json)?;
        }
        Ok(())
    }

    pub async fn get_auth_url(&mut self) -> Result<String> {
        Ok(self.spotify.get_authorize_url(None)?)
    }

    /// Spins up a one-shot HTTP server on port 8888 to catch the OAuth redirect.
    /// Runs the blocking tiny_http loop on a dedicated thread so the async
    /// runtime is never blocked.
    pub async fn wait_for_auth_code() -> Result<String> {
        let fut = tokio::task::spawn_blocking(|| -> Result<String> {
            let server = tiny_http::Server::http("127.0.0.1:8888")
                .map_err(|e| anyhow!("{}", e))?;
            for request in server.incoming_requests() {
                let url = request.url().to_owned();
                if url.contains("/callback?code=") {
                    let code = url
                        .split("code=")
                        .nth(1)
                        .and_then(|s| s.split('&').next())
                        .map(|s| s.to_string())
                        .ok_or_else(|| anyhow!("Malformed callback URL: {}", url))?;
                    let _ = request.respond(tiny_http::Response::from_string(
                        "Authentication successful! You can close this window.",
                    ));
                    return Ok(code);
                }
                let _ = request
                    .respond(tiny_http::Response::from_string("Waiting for Spotify…"));
            }
            Err(anyhow!("HTTP server closed before receiving the auth code"))
        });
        match tokio::time::timeout(std::time::Duration::from_secs(120), fut).await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => Err(anyhow!("spawn_blocking failed: {}", e)),
            Err(_) => Err(anyhow!("OAuth timed out after 120 seconds — press 'l' to retry")),
        }
    }

    pub async fn complete_auth(&self, code: &str) -> Result<()> {
        tracing::info!("Requesting Spotify token from authorization code...");
        self.spotify.request_token(code).await?;
        tracing::info!("Token received, saving to disk...");
        self.save_token().await?;
        Ok(())
    }

    /// Fetches all playlists for the current user.
    ///
    /// Uses a raw HTTP + serde_json::Value approach instead of rspotify's
    /// `current_user_playlists()` stream because rspotify 0.13 requires a
    /// `tracks` field in `SimplifiedPlaylist` that Spotify omits on many
    /// auto-generated playlists (Daily Mix, Radio, etc.), causing every item
    /// to fail deserialization.
    pub async fn list_playlists(&self) -> Result<Vec<SpotifyPlaylist>> {
        // Refresh the access token if it has expired.
        // Bind the Arc so the temporary lives long enough for the guard borrow.
        let is_expired = {
            let token_arc = self.spotify.get_token();
            let guard = token_arc.lock().await.unwrap();
            guard.as_ref().map(|t| t.is_expired()).unwrap_or(true)
        };
        if is_expired {
            self.spotify
                .refresh_token()
                .await
                .map_err(|e| anyhow!("Token refresh failed: {}", e))?;
            let _ = self.save_token().await;
        }

        let access_token = {
            let token_arc = self.spotify.get_token();
            let guard = token_arc.lock().await.unwrap();
            guard
                .as_ref()
                .map(|t| t.access_token.clone())
                .ok_or_else(|| anyhow!("No access token — press 'l' to reconnect"))?
        };

        let http = reqwest::Client::new();
        let mut playlists = Vec::new();
        let mut next: Option<String> =
            Some("https://api.spotify.com/v1/me/playlists?limit=50".to_string());

        while let Some(url) = next {
            let resp = http
                .get(&url)
                .header("Authorization", format!("Bearer {}", access_token))
                .send()
                .await
                .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

            let status = resp.status();
            if status.as_u16() == 401 {
                return Err(anyhow!("Unauthorized — token expired. Press 'l' to reconnect"));
            }
            if !status.is_success() {
                return Err(anyhow!("Spotify API returned {}", status));
            }

            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| anyhow!("Failed to parse Spotify response: {}", e))?;

            if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    if item.is_null() {
                        continue;
                    }
                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unnamed Playlist");
                    if !id.is_empty() {
                        playlists.push(SpotifyPlaylist {
                            id: id.to_string(),
                            name: name.to_string(),
                        });
                    }
                }
            }

            // Follow pagination
            next = body
                .get("next")
                .and_then(|v| if v.is_null() { None } else { v.as_str() })
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
        }

        Ok(playlists)
    }

    pub async fn search_tracks(&self, query: &str) -> Result<Vec<rspotify::model::FullTrack>> {
        let results = self.spotify.search(query, rspotify::model::SearchType::Track, None, None, Some(10), None).await?;
        if let rspotify::model::SearchResult::Tracks(page) = results {
            Ok(page.items)
        } else {
            Ok(vec![])
        }
    }
    
    pub async fn get_devices(&self) -> Result<Vec<rspotify::model::Device>> {
        Ok(self.spotify.device().await?)
    }

    pub async fn get_current_playback(&self) -> Result<Option<rspotify::model::CurrentPlaybackContext>> {
        Ok(self.spotify.current_playback(None, None::<Vec<_>>).await?)
    }

    /// Returns true if a token (fresh or expired) is currently stored in the client.
    pub async fn has_token(&self) -> bool {
        self.spotify.get_token().lock().await.unwrap().is_some()
    }
}
