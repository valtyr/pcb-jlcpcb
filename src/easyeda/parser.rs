//! Parser for EasyEDA symbol pin data.

use super::Pin;

/// Parse pins from EasyEDA symbol shape data.
///
/// The shape array contains elements like:
/// ```text
/// "P~show~0~1~320~280~180~gge9~0^^320~280^^M 320 280 h 20~#880000^^1~342~283~0~SD_MODE~start~~~#0000FF^^1~335~279~0~A1~end~~~#0000FF^^0~337~280^^0~M 340 283 L 343 280 L 340 277"
/// ```
///
/// Pin elements start with "P~" and contain:
/// - Segment 0: Settings (spice pin number at index 3)
/// - Segment 3: Pin name (at index 4)
/// - Segment 4: Display pin number (at index 4)
pub fn parse_symbol_pins(shapes: &[String]) -> Vec<Pin> {
    let mut pins = Vec::new();

    for shape in shapes {
        // Only process pin elements
        if !shape.starts_with("P~") {
            continue;
        }

        if let Some(pin) = parse_pin_shape(shape) {
            pins.push(pin);
        }
    }

    // Sort pins by number (alphanumeric sort for BGA-style pins like A1, B2)
    pins.sort_by(|a, b| {
        // Try numeric sort first, fall back to alphanumeric
        match (a.number.parse::<u32>(), b.number.parse::<u32>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            _ => alphanum_sort(&a.number, &b.number),
        }
    });

    pins
}

/// Parse a single pin shape element.
fn parse_pin_shape(shape: &str) -> Option<Pin> {
    // Split by ^^ to get segments
    let segments: Vec<&str> = shape.split("^^").collect();

    if segments.len() < 5 {
        return None;
    }

    // Segment 0: Settings - contains spice pin number at index 3
    let settings: Vec<&str> = segments[0].split('~').collect();
    let spice_pin_number = settings.get(3).map(|s| s.to_string());

    // Segment 3: Pin name info
    let name_parts: Vec<&str> = segments[3].split('~').collect();
    let pin_name = name_parts
        .get(4)
        .map(|s| clean_pin_name(s))
        .filter(|s| !s.is_empty());

    // Segment 4: Display pin number
    let number_parts: Vec<&str> = segments[4].split('~').collect();
    let display_pin_number = number_parts
        .get(4)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Prefer display pin number (handles alphanumeric like A1, B2),
    // fall back to spice pin number
    let number = display_pin_number
        .or(spice_pin_number)
        .filter(|s| !s.is_empty())?;

    let name = pin_name?;

    Some(Pin { number, name })
}

/// Simple alphanumeric sort (handles A1, A2, B1, etc.)
fn alphanum_sort(a: &str, b: &str) -> std::cmp::Ordering {
    // Extract letter prefix and numeric suffix
    let (a_prefix, a_num) = split_alphanum(a);
    let (b_prefix, b_num) = split_alphanum(b);

    match a_prefix.cmp(&b_prefix) {
        std::cmp::Ordering::Equal => a_num.cmp(&b_num),
        other => other,
    }
}

/// Split string into letter prefix and numeric suffix.
fn split_alphanum(s: &str) -> (&str, u32) {
    let idx = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let prefix = &s[..idx];
    let num = s[idx..].parse().unwrap_or(0);
    (prefix, num)
}

/// Clean up pin name by removing trailing markers.
fn clean_pin_name(name: &str) -> String {
    name.trim()
        .trim_end_matches('#')
        .trim_end_matches('~')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_max98357_pins() {
        let shapes = vec![
            "P~show~0~1~320~280~180~gge9~0^^320~280^^M 320 280 h 20~#880000^^1~342~283~0~SD_MODE~start~~~#0000FF^^1~335~279~0~A1~end~~~#0000FF^^0~337~280^^0~M 340 283 L 343 280 L 340 277".to_string(),
            "P~show~0~A2~320~290~180~gge16~0^^320~290^^M 320 290 h 20~#880000^^1~342~293~0~VDD~start~~~#0000FF^^1~335~289~0~A2~end~~~#0000FF^^0~337~290^^0~M 340 293 L 343 290 L 340 287".to_string(),
            "R~340~270~2~2~120~70~#880000~1~0~none~gge79~0~".to_string(), // Rectangle, should be skipped
        ];

        let pins = parse_symbol_pins(&shapes);

        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].number, "A1");
        assert_eq!(pins[0].name, "SD_MODE");
        assert_eq!(pins[1].number, "A2");
        assert_eq!(pins[1].name, "VDD");
    }
}
