//! Pin cache management.
//!
//! Caches extracted pin mappings at `~/.pcb/jlcpcb/pins/<lcsc>.json` to avoid
//! repeated API calls for the same component.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::easyeda::{ComponentMeta, Pin};

/// Cached pin information for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPins {
    /// LCSC part number
    pub lcsc: String,
    /// Manufacturer part number
    pub mpn: String,
    /// When the pins were extracted
    pub extracted_at: DateTime<Utc>,
    /// Extracted pin mappings
    pub pins: Vec<Pin>,
    /// Component metadata (footprint, 3D model, etc.)
    #[serde(default)]
    pub meta: Option<ComponentMeta>,
}

/// Pin cache manager.
pub struct PinCache {
    cache_dir: PathBuf,
}

impl Default for PinCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PinCache {
    /// Create a new pin cache manager.
    ///
    /// Cache location: `~/.pcb/jlcpcb/pins/`
    pub fn new() -> Self {
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pcb")
            .join("jlcpcb")
            .join("pins");

        Self { cache_dir }
    }

    /// Create cache with a custom directory (for testing).
    pub fn with_dir(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Get the cache file path for an LCSC part number.
    fn cache_path(&self, lcsc: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.json", lcsc))
    }

    /// Load cached pins for a part.
    pub fn load(&self, lcsc: &str) -> Result<Option<CachedPins>> {
        let path = self.cache_path(lcsc);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache file: {}", path.display()))?;

        let cached: CachedPins = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache file: {}", path.display()))?;

        Ok(Some(cached))
    }

    /// Save pins to cache.
    pub fn save(
        &self,
        lcsc: &str,
        mpn: &str,
        pins: &[Pin],
        meta: Option<&ComponentMeta>,
    ) -> Result<()> {
        // Ensure cache directory exists
        fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", self.cache_dir.display()))?;

        let cached = CachedPins {
            lcsc: lcsc.to_string(),
            mpn: mpn.to_string(),
            extracted_at: Utc::now(),
            pins: pins.to_vec(),
            meta: meta.cloned(),
        };

        let content = serde_json::to_string_pretty(&cached)
            .context("Failed to serialize cache data")?;

        let path = self.cache_path(lcsc);
        fs::write(&path, content)
            .with_context(|| format!("Failed to write cache file: {}", path.display()))?;

        Ok(())
    }

    /// Remove cached pins for a part.
    pub fn remove(&self, lcsc: &str) -> Result<bool> {
        let path = self.cache_path(lcsc);

        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove cache file: {}", path.display()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if a part has cached pins.
    pub fn exists(&self, lcsc: &str) -> bool {
        self.cache_path(lcsc).exists()
    }

    /// Get the cache directory path.
    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let cache = PinCache::with_dir(temp_dir.path().to_path_buf());

        let pins = vec![
            Pin {
                number: "1".to_string(),
                name: "VCC".to_string(),
            },
            Pin {
                number: "2".to_string(),
                name: "GND".to_string(),
            },
        ];

        // Save
        cache.save("C123456", "TEST-PART", &pins, None).unwrap();

        // Load
        let loaded = cache.load("C123456").unwrap().unwrap();
        assert_eq!(loaded.lcsc, "C123456");
        assert_eq!(loaded.mpn, "TEST-PART");
        assert_eq!(loaded.pins.len(), 2);
        assert_eq!(loaded.pins[0].name, "VCC");

        // Exists
        assert!(cache.exists("C123456"));
        assert!(!cache.exists("C999999"));

        // Remove
        assert!(cache.remove("C123456").unwrap());
        assert!(!cache.exists("C123456"));
    }
}
