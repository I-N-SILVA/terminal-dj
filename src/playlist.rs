use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub tracks: Vec<PathBuf>,
}

impl Playlist {
    pub fn new(id: i64, name: String) -> Self {
        Self {
            id,
            name,
            tracks: Vec::new(),
        }
    }
}
