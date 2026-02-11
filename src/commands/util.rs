//! Utility commands (cache management, etc.).

use anyhow::Result;

use crate::api::cache::PartCache;
use crate::pins::cache::PinCache;

/// Clear cached API data.
///
/// When neither `parts` nor `pins` is true, both caches are cleared.
pub fn execute_clean_cache(parts: bool, pins: bool) -> Result<()> {
    let clean_both = !parts && !pins;

    if clean_both || parts {
        let cache = PartCache::new();
        match cache.clear() {
            Ok((count, dir)) => {
                println!("Cleared part cache: {} file(s) removed ({})", count, dir.display());
            }
            Err(e) => {
                eprintln!("Failed to clear part cache: {}", e);
            }
        }
    }

    if clean_both || pins {
        let cache = PinCache::new();
        match cache.clear() {
            Ok((count, dir)) => {
                println!("Cleared pin cache: {} file(s) removed ({})", count, dir.display());
            }
            Err(e) => {
                eprintln!("Failed to clear pin cache: {}", e);
            }
        }
    }

    Ok(())
}
