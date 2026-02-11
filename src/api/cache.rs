//! Part cache with TTL.
//!
//! Caches JLCPCB part lookups at `~/.pcb/jlcpcb/parts/<lcsc>.json` to avoid
//! repeated API calls. Entries expire after 24 hours (checked via file mtime).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api::types::JlcPart;

/// Cached part data with a time-to-live based on file modification time.
pub struct PartCache {
    cache_dir: PathBuf,
    ttl: Duration,
}

impl Default for PartCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PartCache {
    /// Create a new part cache.
    ///
    /// Cache location: `~/.pcb/jlcpcb/parts/`, TTL: 24 hours.
    pub fn new() -> Self {
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pcb")
            .join("jlcpcb")
            .join("parts");

        Self {
            cache_dir,
            ttl: Duration::from_secs(24 * 60 * 60),
        }
    }

    /// Load a cached part if it exists and hasn't expired.
    pub fn load(&self, lcsc: &str) -> Option<JlcPart> {
        let path = self.cache_dir.join(format!("{}.json", lcsc));

        let metadata = fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;

        // Check TTL via mtime
        if modified.elapsed().unwrap_or(Duration::MAX) > self.ttl {
            return None;
        }

        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Get the cache directory path.
    pub fn dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Remove all cached part files.
    pub fn clear(&self) -> Result<(usize, PathBuf), std::io::Error> {
        let dir = &self.cache_dir;
        let mut count = 0;

        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    count += 1;
                }
            }
            fs::remove_dir_all(dir)?;
        }

        fs::create_dir_all(dir)?;
        Ok((count, dir.clone()))
    }

    /// Save a part to the cache.
    pub fn save(&self, lcsc: &str, part: &JlcPart) {
        if fs::create_dir_all(&self.cache_dir).is_err() {
            return;
        }

        let path = self.cache_dir.join(format!("{}.json", lcsc));
        if let Ok(content) = serde_json::to_string_pretty(part) {
            let _ = fs::write(&path, content);
        }
    }
}
