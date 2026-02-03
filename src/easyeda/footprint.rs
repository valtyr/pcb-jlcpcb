//! EasyEDA footprint to KiCad .kicad_mod converter.

use anyhow::Result;
use std::fmt::Write;

/// EasyEDA to KiCad coordinate conversion factor.
/// EasyEDA uses 10 mil units, KiCad uses mm.
const EASYEDA_TO_MM: f64 = 0.254;

/// Parsed pad from EasyEDA footprint.
#[derive(Debug, Clone)]
pub struct FootprintPad {
    /// Pad number (e.g., "1", "A1").
    pub number: String,
    /// Pad shape (rect, oval, circle).
    pub shape: PadShape,
    /// Center X in mm.
    pub x: f64,
    /// Center Y in mm.
    pub y: f64,
    /// Width in mm.
    pub width: f64,
    /// Height in mm.
    pub height: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Whether this is a through-hole pad.
    pub through_hole: bool,
    /// Drill hole diameter in mm (for TH pads).
    pub drill: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub enum PadShape {
    Rect,
    Oval,
    Circle,
}

impl PadShape {
    fn to_kicad(&self) -> &'static str {
        match self {
            PadShape::Rect => "rect",
            PadShape::Oval => "oval",
            PadShape::Circle => "circle",
        }
    }
}

/// Parsed track/line from EasyEDA footprint (for silkscreen).
#[derive(Debug, Clone)]
pub struct FootprintLine {
    /// Start X in mm.
    pub x1: f64,
    /// Start Y in mm.
    pub y1: f64,
    /// End X in mm.
    pub x2: f64,
    /// End Y in mm.
    pub y2: f64,
    /// Line width in mm.
    pub width: f64,
    /// Layer (F.SilkS, B.SilkS, etc.).
    pub layer: String,
}

/// Parse EasyEDA footprint shapes into pads and lines.
pub fn parse_footprint_shapes(shapes: &[String]) -> (Vec<FootprintPad>, Vec<FootprintLine>) {
    let mut pads = Vec::new();
    let mut lines = Vec::new();

    for shape in shapes {
        if shape.starts_with("PAD~") {
            if let Some(pad) = parse_pad(shape) {
                pads.push(pad);
            }
        } else if shape.starts_with("TRACK~") {
            lines.extend(parse_track(shape));
        }
    }

    // Sort pads by number
    pads.sort_by(|a, b| {
        match (a.number.parse::<u32>(), b.number.parse::<u32>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            _ => alphanum_cmp(&a.number, &b.number),
        }
    });

    (pads, lines)
}

/// Parse a PAD shape string.
/// Format: PAD~shape~cx~cy~width~height~layer~net~number~holeRad~points~rotation~id~...
fn parse_pad(shape: &str) -> Option<FootprintPad> {
    let parts: Vec<&str> = shape.split('~').collect();
    if parts.len() < 13 {
        return None;
    }

    let shape_type = parts.get(1)?;
    let cx: f64 = parts.get(2)?.parse().ok()?;
    let cy: f64 = parts.get(3)?.parse().ok()?;
    let width: f64 = parts.get(4)?.parse().ok()?;
    let height: f64 = parts.get(5)?.parse().ok()?;
    let layer: i32 = parts.get(6)?.parse().unwrap_or(1);
    let number = parts.get(8)?.to_string();
    let hole_rad: f64 = parts.get(9)?.parse().unwrap_or(0.0);
    let rotation: f64 = parts.get(11)?.parse().unwrap_or(0.0);

    if number.is_empty() {
        return None;
    }

    let pad_shape = match *shape_type {
        "RECT" => PadShape::Rect,
        "OVAL" => PadShape::Oval,
        "ELLIPSE" => {
            if (width - height).abs() < 0.01 {
                PadShape::Circle
            } else {
                PadShape::Oval
            }
        }
        "POLYGON" => PadShape::Rect, // Approximate as rect
        _ => PadShape::Rect,
    };

    // Layer 11 = multi-layer (through-hole), 1 = top, 2 = bottom
    let through_hole = layer == 11 || hole_rad > 0.0;

    Some(FootprintPad {
        number,
        shape: pad_shape,
        x: cx * EASYEDA_TO_MM,
        y: cy * EASYEDA_TO_MM,
        width: width * EASYEDA_TO_MM,
        height: height * EASYEDA_TO_MM,
        rotation,
        through_hole,
        drill: if hole_rad > 0.0 {
            Some(hole_rad * 2.0 * EASYEDA_TO_MM)
        } else {
            None
        },
    })
}

/// Parse a TRACK shape string into line segments.
/// Format: TRACK~width~layer~net~points~id~locked
fn parse_track(shape: &str) -> Vec<FootprintLine> {
    let parts: Vec<&str> = shape.split('~').collect();
    if parts.len() < 5 {
        return Vec::new();
    }

    let width: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.15);
    let layer_id: i32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let points_str = parts.get(4).unwrap_or(&"");

    // Map EasyEDA layer to KiCad layer
    let layer = match layer_id {
        1 => "F.Cu",
        2 => "B.Cu",
        3 | 13 => "F.SilkS", // Top silk
        4 | 14 => "B.SilkS", // Bottom silk
        5 | 15 => "F.Paste",
        6 | 16 => "B.Paste",
        7 | 17 => "F.Mask",
        8 | 18 => "B.Mask",
        10 | 12 => "F.CrtYd",
        _ => "F.SilkS", // Default to silkscreen
    };

    // Only include silkscreen and courtyard for footprints
    if !layer.contains("SilkS") && !layer.contains("CrtYd") {
        return Vec::new();
    }

    let coords: Vec<f64> = points_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();

    let mut lines = Vec::new();
    for chunk in coords.chunks(2).collect::<Vec<_>>().windows(2) {
        if let [p1, p2] = chunk {
            if p1.len() == 2 && p2.len() == 2 {
                lines.push(FootprintLine {
                    x1: p1[0] * EASYEDA_TO_MM,
                    y1: p1[1] * EASYEDA_TO_MM,
                    x2: p2[0] * EASYEDA_TO_MM,
                    y2: p2[1] * EASYEDA_TO_MM,
                    width: width * EASYEDA_TO_MM,
                    layer: layer.to_string(),
                });
            }
        }
    }

    lines
}

/// Generate KiCad .kicad_mod file content.
pub fn generate_kicad_mod(name: &str, pads: &[FootprintPad], lines: &[FootprintLine]) -> Result<String> {
    let mut out = String::new();

    // Calculate center offset (EasyEDA footprints may not be centered)
    let (offset_x, offset_y) = calculate_center_offset(pads);

    writeln!(out, "(footprint \"{}\"", name)?;
    writeln!(out, "  (version 20240108)")?;
    writeln!(out, "  (generator \"pcb-jlcpcb\")")?;
    writeln!(out, "  (generator_version \"1.0\")")?;
    writeln!(out, "  (layer \"F.Cu\")")?;

    // Reference and value text
    writeln!(out, "  (fp_text reference \"REF**\" (at 0 -2) (layer \"F.SilkS\")")?;
    writeln!(out, "    (effects (font (size 1 1) (thickness 0.15)))")?;
    writeln!(out, "  )")?;
    writeln!(out, "  (fp_text value \"{}\" (at 0 2) (layer \"F.Fab\")", name)?;
    writeln!(out, "    (effects (font (size 1 1) (thickness 0.15)))")?;
    writeln!(out, "  )")?;

    // Write pads
    for pad in pads {
        write_pad(&mut out, pad, offset_x, offset_y)?;
    }

    // Write silkscreen lines
    for line in lines {
        write_line(&mut out, line, offset_x, offset_y)?;
    }

    writeln!(out, ")")?;

    Ok(out)
}

/// Calculate offset to center the footprint.
fn calculate_center_offset(pads: &[FootprintPad]) -> (f64, f64) {
    if pads.is_empty() {
        return (0.0, 0.0);
    }

    let min_x = pads.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = pads.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = pads.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = pads.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);

    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    (center_x, center_y)
}

/// Write a single pad to the output.
fn write_pad(out: &mut String, pad: &FootprintPad, offset_x: f64, offset_y: f64) -> Result<()> {
    let x = pad.x - offset_x;
    let y = pad.y - offset_y;

    let pad_type = if pad.through_hole { "thru_hole" } else { "smd" };
    let layers = if pad.through_hole {
        "\"*.Cu\" \"*.Mask\""
    } else {
        "\"F.Cu\" \"F.Paste\" \"F.Mask\""
    };

    write!(
        out,
        "  (pad \"{}\" {} {} (at {:.4} {:.4}",
        pad.number,
        pad_type,
        pad.shape.to_kicad(),
        x,
        y
    )?;

    if pad.rotation.abs() > 0.01 {
        write!(out, " {:.1}", pad.rotation)?;
    }

    write!(out, ") (size {:.4} {:.4})", pad.width, pad.height)?;

    if let Some(drill) = pad.drill {
        write!(out, " (drill {:.4})", drill)?;
    }

    writeln!(out, " (layers {}))", layers)?;

    Ok(())
}

/// Write a single line to the output.
fn write_line(out: &mut String, line: &FootprintLine, offset_x: f64, offset_y: f64) -> Result<()> {
    let x1 = line.x1 - offset_x;
    let y1 = line.y1 - offset_y;
    let x2 = line.x2 - offset_x;
    let y2 = line.y2 - offset_y;

    writeln!(
        out,
        "  (fp_line (start {:.4} {:.4}) (end {:.4} {:.4}) (stroke (width {:.4}) (type solid)) (layer \"{}\"))",
        x1, y1, x2, y2, line.width, line.layer
    )?;

    Ok(())
}

/// Alphanumeric comparison for pad numbers.
fn alphanum_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_prefix, a_num) = split_alphanum(a);
    let (b_prefix, b_num) = split_alphanum(b);

    match a_prefix.cmp(&b_prefix) {
        std::cmp::Ordering::Equal => a_num.cmp(&b_num),
        other => other,
    }
}

fn split_alphanum(s: &str) -> (&str, u32) {
    let idx = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let prefix = &s[..idx];
    let num = s[idx..].parse().unwrap_or(0);
    (prefix, num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_smd_pad() {
        let shape = "PAD~RECT~100~100~10~20~1~~1~~~0~gge1~~~~";
        let pad = parse_pad(shape).unwrap();
        assert_eq!(pad.number, "1");
        assert!(!pad.through_hole);
        assert!((pad.width - 2.54).abs() < 0.01); // 10 * 0.254
        assert!((pad.height - 5.08).abs() < 0.01); // 20 * 0.254
    }

    #[test]
    fn test_parse_through_hole_pad() {
        let shape = "PAD~ELLIPSE~100~100~10~10~11~~1~3~~~0~gge1~~~~";
        let pad = parse_pad(shape).unwrap();
        assert_eq!(pad.number, "1");
        assert!(pad.through_hole);
        assert!(pad.drill.is_some());
    }
}
