//! JLCPCB/LCSC API response types.

use serde::{Deserialize, Serialize};

/// A part from the JLCPCB basic parts library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JlcPart {
    /// LCSC part number (e.g., "C307331")
    pub lcsc: String,
    /// Manufacturer part number
    pub mpn: String,
    /// Manufacturer name
    pub manufacturer: String,
    /// Part category (e.g., "Resistors", "Capacitors")
    pub category: String,
    /// Subcategory (e.g., "Chip Resistors - Surface Mount")
    #[serde(default)]
    pub subcategory: String,
    /// Package/footprint (e.g., "0402", "0603")
    pub package: String,
    /// Part description
    pub description: String,
    /// Stock quantity
    pub stock: i64,
    /// Price breaks (quantity -> unit price in USD)
    #[serde(default)]
    pub price_breaks: Vec<PriceBreak>,
    /// Datasheet URL
    #[serde(default)]
    pub datasheet: Option<String>,
    /// Whether this is a JLCPCB basic part (lower assembly fee)
    #[serde(default)]
    pub basic: bool,
    /// Whether this is a JLCPCB preferred part
    #[serde(default)]
    pub preferred: bool,
    /// Component attributes (parsed from description/specs)
    #[serde(default)]
    pub attributes: PartAttributes,
}

/// Price break for quantity pricing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceBreak {
    /// Minimum quantity for this price
    pub qty: i32,
    /// Unit price in USD
    pub price: f64,
}

/// Parsed component attributes (for resistors, capacitors, etc.).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartAttributes {
    /// Resistance value (e.g., "10k", "4.7R")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resistance: Option<String>,
    /// Capacitance value (e.g., "100nF", "10uF")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacitance: Option<String>,
    /// Inductance value (e.g., "10uH", "100nH")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inductance: Option<String>,
    /// Voltage rating (e.g., "16V", "50V")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<String>,
    /// Power rating (e.g., "0.1W", "0.25W")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Tolerance (e.g., "1%", "5%")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<String>,
    /// Temperature coefficient or dielectric (e.g., "X7R", "C0G", "NP0")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dielectric: Option<String>,
}


impl JlcPart {
    /// Get the unit price at a given quantity.
    pub fn price_at_qty(&self, qty: i32) -> Option<f64> {
        self.price_breaks
            .iter()
            .filter(|pb| pb.qty <= qty)
            .max_by_key(|pb| pb.qty)
            .or_else(|| self.price_breaks.first())
            .map(|pb| pb.price)
    }

    /// Get the LCSC URL for this part.
    pub fn lcsc_url(&self) -> String {
        format!("https://www.lcsc.com/product-detail/{}.html", self.lcsc)
    }

    /// Check if this part matches a category (case-insensitive prefix match).
    pub fn matches_category(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.category.to_lowercase().starts_with(&q)
            || self.subcategory.to_lowercase().starts_with(&q)
    }

    /// Check if this part matches a package (case-insensitive).
    pub fn matches_package(&self, query: &str) -> bool {
        self.package.eq_ignore_ascii_case(query)
    }
}

/// Part type classification for .zen generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartType {
    Resistor,
    Capacitor,
    Inductor,
    Led,
    Diode,
    Transistor,
    Other,
}

impl JlcPart {
    /// Classify this part based on category.
    pub fn part_type(&self) -> PartType {
        let cat = self.category.to_lowercase();
        let subcat = self.subcategory.to_lowercase();

        if cat.contains("resistor") || subcat.contains("resistor") {
            PartType::Resistor
        } else if cat.contains("capacitor") || subcat.contains("capacitor") {
            PartType::Capacitor
        } else if cat.contains("inductor") || subcat.contains("inductor") {
            PartType::Inductor
        } else if cat.contains("led") || subcat.contains("led") {
            PartType::Led
        } else if cat.contains("diode") || subcat.contains("diode") {
            PartType::Diode
        } else if cat.contains("transistor") || subcat.contains("transistor") {
            PartType::Transistor
        } else {
            PartType::Other
        }
    }

    /// Check if this part can use a stdlib generic module.
    pub fn uses_stdlib_generic(&self) -> bool {
        matches!(
            self.part_type(),
            PartType::Resistor | PartType::Capacitor | PartType::Inductor
        )
    }
}
