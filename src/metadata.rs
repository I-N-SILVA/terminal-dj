use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct TrackMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<Duration>,
    pub cover_art: Option<Vec<u8>>,
}

impl TrackMeta {
    pub fn read(path: &Path) -> Self {
        let mut meta = TrackMeta::default();

        let tagged = match Probe::open(path).and_then(|p| p.read()) {
            Ok(t) => t,
            Err(_) => return meta,
        };

        meta.duration = Some(tagged.properties().duration());

        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        if let Some(tag) = tag {
            meta.title = tag.title().map(|s| s.to_string());
            meta.artist = tag.artist().map(|s| s.to_string());
            meta.album = tag.album().map(|s| s.to_string());
            meta.cover_art = tag.pictures().iter().next().map(|p| p.data().to_vec());
        }

        // If no embedded art, look for local files
        if meta.cover_art.is_none() {
            if let Some(parent) = path.parent() {
                if let Ok(entries) = std::fs::read_dir(parent) {
                    let common_keywords =
                        ["cover", "folder", "album", "front", "artwork", "pasted"];
                    for entry in entries.flatten() {
                        let file_name = entry.file_name().to_string_lossy().to_lowercase();
                        let is_image = file_name.ends_with(".jpg")
                            || file_name.ends_with(".jpeg")
                            || file_name.ends_with(".png");
                        if is_image {
                            for kw in common_keywords {
                                if file_name.contains(kw) {
                                    if let Ok(data) = std::fs::read(entry.path()) {
                                        meta.cover_art = Some(data);
                                        return meta; // Found it
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        meta
    }

    pub fn display_title(&self, path: &Path) -> String {
        self.title.clone().unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    pub fn display_artist(&self) -> &str {
        self.artist.as_deref().unwrap_or("Unknown Artist")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn display_title_falls_back_to_stem() {
        let mut p = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        p.push(format!("tdj_test_{}.mp3", ts));
        let _ = File::create(&p).unwrap();
        let meta = TrackMeta::read(&p);
        let title = meta.display_title(&p);
        assert!(title.contains("tdj_test_"));
        let _ = std::fs::remove_file(&p);
    }
}
