//! EasyEDA API client for fetching component data.
//!
//! This module provides access to the EasyEDA/LCSC component library,
//! which contains schematic symbols with accurate pin information.

mod api;
pub mod footprint;
mod parser;
pub mod symbol;

use serde::{Deserialize, Serialize};

pub use api::{ComponentData, EasyEdaClient};
pub use footprint::{generate_kicad_mod, parse_footprint_shapes};
pub use parser::parse_symbol_pins;
pub use symbol::generate_kicad_sym;

/// A component pin with number and name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    /// Pin number (e.g., "1", "A1", "B2")
    pub number: String,
    /// Pin name (e.g., "VCC", "GND", "MOSI")
    pub name: String,
}

/// Component metadata from EasyEDA.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentMeta {
    /// EasyEDA component UUID.
    pub uuid: Option<String>,
    /// Footprint/package name.
    pub footprint_name: Option<String>,
    /// 3D model name (if available).
    pub model_3d: Option<String>,
    /// Raw footprint shapes from EasyEDA (for generating .kicad_mod).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprint_shapes: Vec<String>,
    /// Raw symbol shapes from EasyEDA (for generating .kicad_sym).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_shapes: Vec<String>,
}

impl ComponentMeta {
    /// Extract metadata from EasyEDA component data.
    pub fn from_component_data(data: &ComponentData) -> Self {
        let mut meta = Self {
            uuid: Some(data.uuid.clone()),
            ..Default::default()
        };

        // Extract symbol shapes from component data_str
        if let Some(ref data_str) = data.data_str {
            if let Some(ref shapes) = data_str.shape {
                meta.symbol_shapes = shapes.clone();
            }
        }

        if let Some(ref pkg) = data.package_detail {
            meta.footprint_name = Some(pkg.title.clone());

            if let Some(ref data_str) = pkg.data_str {
                // Extract footprint shapes for later conversion
                if let Some(ref shapes) = data_str.shape {
                    meta.footprint_shapes = shapes.clone();
                }

                if let Some(ref head) = data_str.head {
                    if let Some(ref params) = head.c_para {
                        if meta.footprint_name.is_none() {
                            meta.footprint_name = params.package.clone();
                        }
                        meta.model_3d = params.model_3d.clone();
                    }
                }
            }
        }

        meta
    }

    /// Generate KiCad .kicad_mod file content from stored footprint shapes.
    pub fn generate_footprint(&self) -> Option<String> {
        let name = self.footprint_name.as_ref()?;
        if self.footprint_shapes.is_empty() {
            return None;
        }

        let (pads, lines) = parse_footprint_shapes(&self.footprint_shapes);
        if pads.is_empty() {
            return None;
        }

        generate_kicad_mod(name, &pads, &lines).ok()
    }

    /// Generate KiCad .kicad_sym file content from stored symbol shapes.
    pub fn generate_symbol(&self, name: &str, pins: &[Pin]) -> Option<String> {
        generate_kicad_sym(name, pins, &self.symbol_shapes).ok()
    }

    /// Get EasyEDA component URL.
    pub fn easyeda_url(&self) -> Option<String> {
        self.uuid
            .as_ref()
            .map(|uuid| format!("https://easyeda.com/component/{}", uuid))
    }
}
