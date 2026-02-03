//! EasyEDA symbol to KiCad .kicad_sym converter.

use anyhow::Result;
use std::fmt::Write;

use super::Pin;

/// EasyEDA to KiCad coordinate conversion factor.
/// EasyEDA uses 10 mil units, KiCad uses mm.
const EASYEDA_TO_MM: f64 = 0.254;

/// Parsed rectangle from EasyEDA symbol.
#[derive(Debug, Clone)]
struct SymbolRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Parsed pin with position from EasyEDA symbol.
#[derive(Debug, Clone)]
struct SymbolPin {
    number: String,
    name: String,
    x: f64,
    y: f64,
    rotation: f64,
    length: f64,
}

/// Parse symbol shapes to extract rectangles and pin positions.
fn parse_symbol_shapes(shapes: &[String]) -> (Vec<SymbolRect>, Vec<SymbolPin>) {
    let mut rects = Vec::new();
    let mut pins = Vec::new();

    for shape in shapes {
        if shape.starts_with("R~") {
            if let Some(rect) = parse_rect(shape) {
                rects.push(rect);
            }
        } else if shape.starts_with("P~") {
            if let Some(pin) = parse_pin_with_position(shape) {
                pins.push(pin);
            }
        }
    }

    (rects, pins)
}

/// Parse a rectangle shape.
/// Format: R~x~y~?~?~width~height~...
fn parse_rect(shape: &str) -> Option<SymbolRect> {
    let parts: Vec<&str> = shape.split('~').collect();
    if parts.len() < 7 {
        return None;
    }

    let x: f64 = parts.get(1)?.parse().ok()?;
    let y: f64 = parts.get(2)?.parse().ok()?;
    let width: f64 = parts.get(5)?.parse().ok()?;
    let height: f64 = parts.get(6)?.parse().ok()?;

    Some(SymbolRect {
        x: x * EASYEDA_TO_MM,
        y: y * EASYEDA_TO_MM,
        width: width * EASYEDA_TO_MM,
        height: height * EASYEDA_TO_MM,
    })
}

/// Parse a pin shape with position info.
/// Format: P~show~0~spice_num~x~y~rotation~id~...^^...^^...^^name_segment^^number_segment^^...
fn parse_pin_with_position(shape: &str) -> Option<SymbolPin> {
    let segments: Vec<&str> = shape.split("^^").collect();
    if segments.len() < 5 {
        return None;
    }

    // Segment 0: Settings including position
    let settings: Vec<&str> = segments[0].split('~').collect();
    if settings.len() < 7 {
        return None;
    }

    let x: f64 = settings.get(4)?.parse().ok()?;
    let y: f64 = settings.get(5)?.parse().ok()?;
    let rotation: f64 = settings.get(6)?.parse().unwrap_or(0.0);

    // Segment 3: Pin name
    let name_parts: Vec<&str> = segments[3].split('~').collect();
    let name = name_parts
        .get(4)
        .map(|s| s.trim().trim_end_matches('#').trim_end_matches('~').to_string())
        .filter(|s| !s.is_empty())?;

    // Segment 4: Pin number
    let number_parts: Vec<&str> = segments[4].split('~').collect();
    let number = number_parts
        .get(4)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;

    Some(SymbolPin {
        number,
        name,
        x: x * EASYEDA_TO_MM,
        y: y * EASYEDA_TO_MM,
        rotation,
        length: 2.54, // Standard KiCad pin length
    })
}

/// Generate KiCad .kicad_sym file content.
pub fn generate_kicad_sym(name: &str, pins: &[Pin], shapes: &[String]) -> Result<String> {
    let mut out = String::new();

    // Parse shapes for positions
    let (_rects, symbol_pins) = parse_symbol_shapes(shapes);

    // Create a map of pin number -> position
    let pin_positions: std::collections::HashMap<&str, &SymbolPin> = symbol_pins
        .iter()
        .map(|p| (p.number.as_str(), p))
        .collect();

    // Calculate bounding box from pins
    let (raw_min_x, raw_max_x, raw_min_y, raw_max_y) = calculate_bounds(&symbol_pins);

    // Center offset to move symbol to origin
    let center_x = (raw_min_x + raw_max_x) / 2.0;
    let center_y = (raw_min_y + raw_max_y) / 2.0;

    // Centered bounds
    let min_x = raw_min_x - center_x;
    let max_x = raw_max_x - center_x;
    let min_y = raw_min_y - center_y;
    let max_y = raw_max_y - center_y;

    let box_margin = 2.54;

    writeln!(out, "(kicad_symbol_lib")?;
    writeln!(out, "  (version 20231120)")?;
    writeln!(out, "  (generator \"pcb-jlcpcb\")")?;
    writeln!(out, "  (generator_version \"1.0\")")?;
    writeln!(out, "  (symbol \"{name}\"")?;
    writeln!(out, "    (pin_names (offset 1.016))")?;
    writeln!(out, "    (exclude_from_sim no)")?;
    writeln!(out, "    (in_bom yes)")?;
    writeln!(out, "    (on_board yes)")?;

    // Properties
    writeln!(out, "    (property \"Reference\" \"U\" (at 0 {} 0)", max_y + box_margin + 1.27)?;
    writeln!(out, "      (effects (font (size 1.27 1.27)))")?;
    writeln!(out, "    )")?;
    writeln!(out, "    (property \"Value\" \"{name}\" (at 0 {} 0)", min_y - box_margin - 1.27)?;
    writeln!(out, "      (effects (font (size 1.27 1.27)))")?;
    writeln!(out, "    )")?;
    writeln!(out, "    (property \"Footprint\" \"\" (at 0 0 0)")?;
    writeln!(out, "      (effects (font (size 1.27 1.27)) hide)")?;
    writeln!(out, "    )")?;
    writeln!(out, "    (property \"Datasheet\" \"\" (at 0 0 0)")?;
    writeln!(out, "      (effects (font (size 1.27 1.27)) hide)")?;
    writeln!(out, "    )")?;

    // Symbol body (rectangle)
    writeln!(out, "    (symbol \"{name}_0_1\"")?;
    writeln!(
        out,
        "      (rectangle (start {:.4} {:.4}) (end {:.4} {:.4})",
        min_x - box_margin,
        max_y + box_margin,
        max_x + box_margin,
        min_y - box_margin
    )?;
    writeln!(out, "        (stroke (width 0.254) (type default))")?;
    writeln!(out, "        (fill (type background))")?;
    writeln!(out, "      )")?;
    writeln!(out, "    )")?;

    // Symbol pins
    writeln!(out, "    (symbol \"{name}_1_1\"")?;

    for pin in pins {
        // Try to get position from parsed shapes, or calculate default
        let (pin_x, pin_y, angle) = if let Some(sp) = pin_positions.get(pin.number.as_str()) {
            // Apply centering offset to pin position
            let centered_y = sp.y - center_y;

            // Determine which side of the box this pin is on based on rotation
            let (x, y, a) = match sp.rotation as i32 {
                0 => (max_x + box_margin + 2.54, centered_y, 180.0),   // Right side, points left
                90 => (sp.x - center_x, min_y - box_margin - 2.54, 90.0),   // Bottom, points up
                180 => (min_x - box_margin - 2.54, centered_y, 0.0),   // Left side, points right
                270 => (sp.x - center_x, max_y + box_margin + 2.54, 270.0), // Top, points down
                _ => (max_x + box_margin + 2.54, centered_y, 180.0),
            };
            (x, y, a)
        } else {
            // Default position: stack on the left
            let idx = pins.iter().position(|p| p.number == pin.number).unwrap_or(0);
            let y = max_y - (idx as f64 * 2.54);
            (min_x - box_margin - 2.54, y, 0.0)
        };

        write_pin(&mut out, &pin.number, &pin.name, pin_x, pin_y, angle)?;
    }

    writeln!(out, "    )")?;
    writeln!(out, "  )")?;
    writeln!(out, ")")?;

    Ok(out)
}

/// Calculate bounding box from pin positions.
fn calculate_bounds(pins: &[SymbolPin]) -> (f64, f64, f64, f64) {
    if pins.is_empty() {
        return (-5.08, 5.08, -5.08, 5.08);
    }

    let min_x = pins.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = pins.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = pins.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = pins.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);

    // Ensure minimum size
    let width = (max_x - min_x).max(5.08);
    let height = (max_y - min_y).max(5.08);

    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    (
        center_x - width / 2.0,
        center_x + width / 2.0,
        center_y - height / 2.0,
        center_y + height / 2.0,
    )
}

/// Write a single pin to the output.
fn write_pin(out: &mut String, number: &str, name: &str, x: f64, y: f64, angle: f64) -> Result<()> {
    // Determine pin type based on name
    let pin_type = if name.contains("VCC") || name.contains("VDD") || name.contains("VIN") {
        "power_in"
    } else if name.contains("GND") || name.contains("VSS") {
        "power_in"
    } else if name.contains("OUT") {
        "output"
    } else if name.contains("IN") || name.contains("DIN") {
        "input"
    } else if name.contains("CLK") || name.contains("BCLK") || name.contains("LRCLK") {
        "input"
    } else {
        "bidirectional"
    };

    writeln!(
        out,
        "      (pin {pin_type} line (at {x:.4} {y:.4} {angle:.0}) (length 2.54)"
    )?;
    writeln!(out, "        (name \"{name}\" (effects (font (size 1.27 1.27))))")?;
    writeln!(out, "        (number \"{number}\" (effects (font (size 1.27 1.27))))")?;
    writeln!(out, "      )")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_symbol() {
        let pins = vec![
            Pin { number: "1".to_string(), name: "GND".to_string() },
            Pin { number: "2".to_string(), name: "VCC".to_string() },
        ];
        let result = generate_kicad_sym("TEST", &pins, &[]).unwrap();
        assert!(result.contains("(symbol \"TEST\""));
        assert!(result.contains("GND"));
        assert!(result.contains("VCC"));
    }
}
