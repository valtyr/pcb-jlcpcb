//! Pin extraction from EasyEDA library.

use anyhow::{Context, Result};
use colored::Colorize;

use super::cache::PinCache;
use crate::api::JlcPart;
use crate::easyeda::{parse_symbol_pins, ComponentMeta, EasyEdaClient, Pin};

/// Options for pin extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractionOptions {
    /// Ignore cache, re-fetch pins from EasyEDA
    pub refresh: bool,
}

/// Result of pin extraction including metadata.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Extracted pins.
    pub pins: Vec<Pin>,
    /// Component metadata (footprint, 3D model, etc.).
    pub meta: ComponentMeta,
}

/// Extract pins for a component from EasyEDA library.
///
/// Flow:
/// 1. Check cache (unless --refresh)
/// 2. Fetch from EasyEDA API
/// 3. Cache the result
pub fn extract_pins(part: &JlcPart, options: &ExtractionOptions) -> Result<ExtractionResult> {
    let cache = PinCache::new();

    // Check cache first (unless refresh requested)
    if !options.refresh {
        if let Some(cached) = cache.load(&part.lcsc)? {
            eprintln!(
                "  {} Using cached pins for {}",
                "→".cyan(),
                part.lcsc.green()
            );
            return Ok(ExtractionResult {
                pins: cached.pins,
                meta: cached.meta.unwrap_or_default(),
            });
        }
    }

    // Fetch from EasyEDA API
    let result = extract_via_easyeda(part)?;

    if result.pins.is_empty() {
        anyhow::bail!(
            "No pin information found for {} ({})\n\n\
            The component may not have a symbol in the EasyEDA library.",
            part.lcsc,
            part.mpn
        );
    }

    eprintln!(
        "  {} Extracted {} pins from EasyEDA library",
        "✓".green(),
        result.pins.len()
    );

    // Cache the result
    if let Err(e) = cache.save(&part.lcsc, &part.mpn, &result.pins, Some(&result.meta)) {
        eprintln!("  {} Failed to cache pins: {}", "!".yellow(), e);
    }

    Ok(result)
}

/// Extract pins from EasyEDA library.
fn extract_via_easyeda(part: &JlcPart) -> Result<ExtractionResult> {
    let easyeda = EasyEdaClient::new()?;

    let component = easyeda
        .get_component(&part.lcsc)?
        .context("Component not found in EasyEDA")?;

    let meta = ComponentMeta::from_component_data(&component);

    let shapes = component
        .data_str
        .and_then(|d| d.shape)
        .unwrap_or_default();

    let pins = parse_symbol_pins(&shapes);

    Ok(ExtractionResult { pins, meta })
}
