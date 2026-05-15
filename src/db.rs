use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn new() -> Result<Self> {
        let mut db_path = dirs::config_dir().context("Could not find config directory")?;
        db_path.push("terminal-dj");
        std::fs::create_dir_all(&db_path)?;
        db_path.push("library.db");

        let conn = Connection::open(db_path)?;
        let db = Db { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlist_tracks (
                id INTEGER PRIMARY KEY,
                playlist_id INTEGER NOT NULL,
                track_path TEXT NOT NULL,
                position INTEGER NOT NULL,
                FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
            )",
            [],
        )?;

        Ok(())
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO playlists (name) VALUES (?)",
            params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_playlists(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, name FROM playlists ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut playlists = Vec::new();
        for playlist in rows {
            playlists.push(playlist?);
        }
        Ok(playlists)
    }

    pub fn add_track(&self, playlist_id: i64, track_path: &Path) -> Result<()> {
        // Get current max position
        let mut stmt = self.conn.prepare("SELECT MAX(position) FROM playlist_tracks WHERE playlist_id = ?")?;
        let max_pos: Option<i32> = stmt.query_row(params![playlist_id], |row| row.get(0)).ok().flatten();
        let next_pos = max_pos.unwrap_or(-1) + 1;

        self.conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_path, position) VALUES (?, ?, ?)",
            params![playlist_id, track_path.to_string_lossy(), next_pos],
        )?;
        Ok(())
    }

    pub fn get_tracks(&self, playlist_id: i64) -> Result<Vec<PathBuf>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_path FROM playlist_tracks WHERE playlist_id = ? ORDER BY position"
        )?;
        let rows = stmt.query_map(params![playlist_id], |row| {
            let path_str: String = row.get(0)?;
            Ok(PathBuf::from(path_str))
        })?;

        let mut tracks = Vec::new();
        for track in rows {
            tracks.push(track?);
        }
        Ok(tracks)
    }
}
