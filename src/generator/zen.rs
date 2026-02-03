//! .zen file generation for JLCPCB parts.

use anyhow::{Context, Result};
use minijinja::Environment;
use regex::Regex;

use crate::api::{JlcPart, PartType};

/// Context for rendering the generic passive template.
#[derive(Debug, serde::Serialize)]
struct GenericContext {
    lcsc: String,
    mpn: String,
    manufacturer: String,
    description: String,
    component_type: String,
    name: String,
    value: String,
    package: String,
    tolerance: Option<String>,
    voltage: Option<String>,
    power: Option<String>,
    dielectric: Option<String>,
    pin1: String,
    pin2: String,
}

/// Attributes extracted from a part description.
#[derive(Debug, Default)]
struct ExtractedAttributes {
    voltage: Option<String>,
    tolerance: Option<String>,
    dielectric: Option<String>,
    power: Option<String>,
}

/// Extract attributes from a part description.
fn extract_attributes_from_description(desc: &str) -> ExtractedAttributes {
    let mut attrs = ExtractedAttributes::default();

    // Extract voltage (e.g., "16V", "50V", "25V")
    if let Some(cap) = Regex::new(r"\b(\d+(?:\.\d+)?)\s*V\b")
        .ok()
        .and_then(|re| re.captures(desc))
    {
        attrs.voltage = Some(format!("{}V", &cap[1]));
    }

    // Extract tolerance (e.g., "±10%", "±5%", "1%")
    if let Some(cap) = Regex::new(r"[±]?(\d+(?:\.\d+)?)\s*%")
        .ok()
        .and_then(|re| re.captures(desc))
    {
        attrs.tolerance = Some(format!("{}%", &cap[1]));
    }

    // Extract dielectric for capacitors (e.g., "X7R", "X5R", "C0G", "NP0")
    if let Some(cap) = Regex::new(r"\b(X[57][RSTUV]|C0G|NP0|Y5V)\b")
        .ok()
        .and_then(|re| re.captures(desc))
    {
        attrs.dielectric = Some(cap[1].to_string());
    }

    // Extract power rating (e.g., "0.1W", "1/4W", "0.25W")
    if let Some(cap) = Regex::new(r"\b(\d+(?:\.\d+)?)\s*W\b")
        .ok()
        .and_then(|re| re.captures(desc))
    {
        attrs.power = Some(format!("{}W", &cap[1]));
    }

    attrs
}

/// Context for rendering the component template.
#[derive(Debug, serde::Serialize)]
struct ComponentContext {
    lcsc: String,
    mpn: String,
    manufacturer: String,
    description: String,
    basic: bool,
    name: String,
    /// Unique struct fields for Pins struct (deduplicated by name)
    struct_fields: Vec<StructField>,
    /// All pins with number -> sanitized name mapping
    pins: Vec<PinInfo>,
    datasheet: Option<String>,
    /// Footprint/package name from EasyEDA
    footprint_name: Option<String>,
    /// Footprint filename (e.g., "AMS1117-3_3.kicad_mod")
    footprint_file: Option<String>,
    /// Symbol filename (e.g., "AMS1117-3_3.kicad_sym")
    symbol_file: Option<String>,
    /// 3D model name (if available)
    model_3d: Option<String>,
    /// EasyEDA component URL
    easyeda_url: Option<String>,
}

/// A pin with its number and sanitized name for struct field.
#[derive(Debug, serde::Serialize)]
struct PinInfo {
    /// Pin number (for the pins dict key)
    number: String,
    /// Original pin name
    name: String,
    /// Sanitized name for struct field
    sanitized: String,
}

/// Unique struct field for the Pins struct.
#[derive(Debug, serde::Serialize)]
struct StructField {
    /// Sanitized name for struct field
    sanitized: String,
}

/// Generator for .zen files from JLCPCB parts.
pub struct ZenGenerator {
    env: Environment<'static>,
}

impl Default for ZenGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ZenGenerator {
    /// Create a new generator.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.add_template("generic", include_str!("../../templates/generic.zen.jinja"))
            .expect("Failed to add generic template");
        env.add_template(
            "component",
            include_str!("../../templates/component.zen.jinja"),
        )
        .expect("Failed to add component template");
        Self { env }
    }

    /// Generate a .zen file for a generic passive component.
    pub fn generate_generic(&self, part: &JlcPart, name: &str, pins: (&str, &str)) -> Result<String> {
        let component_type = match part.part_type() {
            PartType::Resistor => "Resistor",
            PartType::Capacitor => "Capacitor",
            PartType::Inductor => "Inductor",
            _ => return Err(anyhow::anyhow!("Part is not a generic passive")),
        };

        let value = extract_value(part);

        // Extract attributes from description if not already set
        let extracted = extract_attributes_from_description(&part.description);

        let ctx = GenericContext {
            lcsc: part.lcsc.clone(),
            mpn: part.mpn.clone(),
            manufacturer: part.manufacturer.clone(),
            description: part.description.clone(),
            component_type: component_type.to_string(),
            name: name.to_string(),
            value,
            package: part.package.clone(),
            tolerance: part.attributes.tolerance.clone().or(extracted.tolerance),
            voltage: part.attributes.voltage.clone().or(extracted.voltage),
            power: part.attributes.power.clone().or(extracted.power),
            dielectric: part.attributes.dielectric.clone().or(extracted.dielectric),
            pin1: pins.0.to_string(),
            pin2: pins.1.to_string(),
        };

        let template = self.env.get_template("generic")?;
        template
            .render(&ctx)
            .context("Failed to render generic template")
    }

    /// Generate a .zen file for a component (non-generic).
    ///
    /// Takes a list of (pin_number, pin_name) tuples and component metadata.
    pub fn generate_component(
        &self,
        part: &JlcPart,
        name: &str,
        pins: &[(String, String)], // (number, name) tuples
        meta: &crate::easyeda::ComponentMeta,
        footprint_file: &Option<String>,
        symbol_file: &Option<String>,
    ) -> Result<String> {
        use std::collections::HashSet;

        // Build list of all pins with their info
        let pin_infos: Vec<PinInfo> = pins
            .iter()
            .map(|(number, pin_name)| PinInfo {
                number: number.clone(),
                name: pin_name.clone(),
                sanitized: sanitize_pin_name(pin_name),
            })
            .collect();

        // Deduplicate struct fields (multiple pins can have the same name, like VOUT on AMS1117)
        let mut seen: HashSet<String> = HashSet::new();
        let struct_fields: Vec<StructField> = pin_infos
            .iter()
            .filter_map(|p| {
                if seen.insert(p.sanitized.clone()) {
                    Some(StructField {
                        sanitized: p.sanitized.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let ctx = ComponentContext {
            lcsc: part.lcsc.clone(),
            mpn: part.mpn.clone(),
            manufacturer: part.manufacturer.clone(),
            description: truncate_description(&part.description),
            basic: part.basic,
            name: name.to_string(),
            struct_fields,
            pins: pin_infos,
            datasheet: part.datasheet.clone(),
            footprint_name: meta.footprint_name.clone(),
            footprint_file: footprint_file.clone(),
            symbol_file: symbol_file.clone(),
            model_3d: meta.model_3d.clone(),
            easyeda_url: meta.easyeda_url(),
        };

        let template = self.env.get_template("component")?;
        template
            .render(&ctx)
            .context("Failed to render component template")
    }
}

/// Extract the value from a part's description or attributes.
fn extract_value(part: &JlcPart) -> String {
    match part.part_type() {
        PartType::Resistor => {
            if let Some(ref res) = part.attributes.resistance {
                return res.clone();
            }
            // Try to extract from description
            if let Some(cap) = extract_resistance_from_desc(&part.description) {
                return cap;
            }
            "—".to_string()
        }
        PartType::Capacitor => {
            if let Some(ref cap) = part.attributes.capacitance {
                return cap.clone();
            }
            if let Some(cap) = extract_capacitance_from_desc(&part.description) {
                return cap;
            }
            "—".to_string()
        }
        PartType::Inductor => {
            if let Some(ref ind) = part.attributes.inductance {
                return ind.clone();
            }
            if let Some(ind) = extract_inductance_from_desc(&part.description) {
                return ind;
            }
            "—".to_string()
        }
        _ => "—".to_string(),
    }
}

/// Extract resistance value from description.
fn extract_resistance_from_desc(desc: &str) -> Option<String> {
    // Match patterns like "10kΩ", "4.7k", "100R", "4R7", "10k"
    let patterns = [
        r"(\d+(?:\.\d+)?)\s*([kKmM]?)[Ωohm]",
        r"(\d+(?:\.\d+)?)\s*([kKmMrR])\s*$",
        r"(\d+)[rR](\d+)",
    ];

    for pattern in patterns {
        if let Some(caps) = Regex::new(pattern).unwrap().captures(desc) {
            let value = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let unit = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if !value.is_empty() {
                let unit = unit.to_lowercase().replace('r', "Ω");
                return Some(format!("{}{}", value, unit));
            }
        }
    }
    None
}

/// Extract capacitance value from description.
fn extract_capacitance_from_desc(desc: &str) -> Option<String> {
    // Match patterns like "100nF", "10uF", "10µF", "1pF"
    let pattern = r"(\d+(?:\.\d+)?)\s*([nuμµp])[fF]";
    if let Some(caps) = Regex::new(pattern).unwrap().captures(desc) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let unit = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        if !value.is_empty() {
            let unit = match unit {
                "μ" | "µ" => "u",
                other => other,
            };
            return Some(format!("{}{}F", value, unit));
        }
    }
    None
}

/// Extract inductance value from description.
fn extract_inductance_from_desc(desc: &str) -> Option<String> {
    // Match patterns like "10uH", "100nH", "1mH"
    let pattern = r"(\d+(?:\.\d+)?)\s*([nuμµm])[hH]";
    if let Some(caps) = Regex::new(pattern).unwrap().captures(desc) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let unit = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        if !value.is_empty() {
            let unit = match unit {
                "μ" | "µ" => "u",
                other => other,
            };
            return Some(format!("{}{}H", value, unit));
        }
    }
    None
}

/// Sanitize a pin name for use as a Starlark identifier.
fn sanitize_pin_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(len + 8);

    for (i, &c) in chars.iter().enumerate() {
        let is_last = i == len - 1;

        match c {
            '+' if is_last => result.push_str("_POS"),
            '-' if is_last => result.push_str("_NEG"),
            '+' | '-' => result.push('_'),
            '~' | '!' => result.push_str("N_"),
            '#' => result.push('H'),
            c if c.is_alphanumeric() => result.push(c.to_ascii_uppercase()),
            _ => result.push('_'),
        }
    }

    // Remove consecutive underscores and trim
    let sanitized = result
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    // Prefix with P if starts with digit
    if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("P{}", sanitized)
    } else if sanitized.is_empty() {
        "PIN".to_string()
    } else {
        sanitized
    }
}

/// Truncate description to a reasonable length.
fn truncate_description(desc: &str) -> String {
    let max_len = 100;
    if desc.len() <= max_len {
        desc.to_string()
    } else {
        format!("{}...", &desc[..max_len - 3])
    }
}

/// Sanitize an MPN for use as a filename.
pub fn sanitize_mpn(mpn: &str) -> String {
    mpn.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_pin_name() {
        assert_eq!(sanitize_pin_name("VCC"), "VCC");
        assert_eq!(sanitize_pin_name("V+"), "V_POS");
        assert_eq!(sanitize_pin_name("V-"), "V_NEG");
        assert_eq!(sanitize_pin_name("~CS"), "N_CS");
        assert_eq!(sanitize_pin_name("1"), "P1");
        assert_eq!(sanitize_pin_name("GND"), "GND");
    }

    #[test]
    fn test_sanitize_mpn() {
        assert_eq!(sanitize_mpn("CL05B104KO5NNNC"), "CL05B104KO5NNNC");
        assert_eq!(sanitize_mpn("STM32F103C8T6"), "STM32F103C8T6");
        assert_eq!(sanitize_mpn("Part/Number"), "Part_Number");
    }

    #[test]
    fn test_extract_capacitance() {
        assert_eq!(
            extract_capacitance_from_desc("100nF 16V X7R"),
            Some("100nF".to_string())
        );
        assert_eq!(
            extract_capacitance_from_desc("10uF 25V"),
            Some("10uF".to_string())
        );
    }
}
