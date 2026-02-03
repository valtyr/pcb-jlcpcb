//! JLCPCB/LCSC API client.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Deserializer, Serialize};

use super::types::{JlcPart, PartAttributes, PriceBreak};

/// JLCPCB API endpoint for component search.
const JLCPCB_SEARCH_URL: &str =
    "https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList/v2";

/// JLCPCB API endpoint for component details.
const JLCPCB_DETAIL_URL: &str =
    "https://cart.jlcpcb.com/shoppingCart/smtGood/getComponentDetail";

/// Secret key required by JLCPCB API.
const JLCPCB_SECRET_KEY: &str = "64656661756c744b65794964";

/// Client for JLCPCB API.
pub struct JlcpcbClient {
    client: Client,
}

/// Library type filter for parts search.
#[derive(Debug, Clone, Copy, Default)]
pub enum LibraryType {
    /// All parts (no filter)
    #[default]
    All,
    /// Basic parts only (lower assembly fee)
    Basic,
    /// Basic + preferred promotional parts
    BasicAndPreferred,
}

/// JLCPCB API search request body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbSearchRequest {
    current_page: i32,
    page_size: i32,
    search_type: i32,
    keyword: String,
    component_library_type: String,
    presale_type: String,
    preferred_component_flag: bool,
    stock_flag: Option<bool>,
    stock_sort: Option<String>,
    first_sort_name: Option<String>,
    second_sort_name: Option<String>,
    component_brand: Option<String>,
    component_specification: Option<String>,
    component_attributes: Vec<String>,
    first_sort_name_list: Vec<String>,
    component_brand_list: Vec<String>,
    component_specification_list: Vec<String>,
    component_attribute_list: Vec<String>,
    search_source: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    component_lib_types: Vec<String>,
}

impl JlcpcbSearchRequest {
    fn new(keyword: &str, page: i32, page_size: i32, library_type: LibraryType) -> Self {
        let (component_library_type, component_lib_types, preferred_component_flag) =
            match library_type {
                LibraryType::All => (String::new(), vec![], false),
                LibraryType::Basic => ("base".to_string(), vec!["base".to_string()], false),
                LibraryType::BasicAndPreferred => {
                    ("base".to_string(), vec!["base".to_string()], true)
                }
            };

        Self {
            current_page: page,
            page_size,
            search_type: 2,
            keyword: keyword.to_string(),
            component_library_type,
            presale_type: String::new(),
            preferred_component_flag,
            stock_flag: None,
            stock_sort: None,
            first_sort_name: None,
            second_sort_name: None,
            component_brand: None,
            component_specification: None,
            component_attributes: Vec::new(),
            first_sort_name_list: Vec::new(),
            component_brand_list: Vec::new(),
            component_specification_list: Vec::new(),
            component_attribute_list: Vec::new(),
            search_source: "search".to_string(),
            component_lib_types,
        }
    }
}

/// JLCPCB API search response.
#[derive(Debug, Deserialize)]
struct JlcpcbSearchResponse {
    code: i32,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<JlcpcbSearchData>,
}

/// JLCPCB search data wrapper.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbSearchData {
    #[serde(default)]
    component_page_info: Option<JlcpcbPageInfo>,
}

/// JLCPCB pagination info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbPageInfo {
    /// Total number of parts matching the query
    #[serde(default)]
    total: i64,
    #[serde(default, deserialize_with = "deserialize_null_as_empty")]
    list: Vec<JlcpcbComponent>,
}

/// Result of a paginated search including total count.
pub struct SearchPage {
    /// Parts returned in this page
    pub parts: Vec<JlcPart>,
    /// Total number of parts matching the query
    pub total: i64,
}

/// Deserialize null as empty vector.
fn deserialize_null_as_empty<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt: Option<Vec<T>> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// JLCPCB component from API response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbComponent {
    /// LCSC part number (e.g., "C307331")
    #[serde(default)]
    component_code: String,
    /// Manufacturer part number
    #[serde(default)]
    component_model_en: String,
    /// Manufacturer name
    #[serde(default)]
    component_brand_en: String,
    /// First level category
    #[serde(default)]
    first_sort_name: String,
    /// Second level category
    #[serde(default)]
    second_sort_name: String,
    /// Package/footprint
    #[serde(default)]
    component_specification: String,
    /// Description
    #[serde(default)]
    describe: String,
    /// Stock quantity
    #[serde(default)]
    stock_count: i64,
    /// Price info
    #[serde(default)]
    component_prices: Vec<JlcpcbPrice>,
    /// Datasheet URL
    #[serde(default)]
    datasheet_url: Option<String>,
    /// Component library type ("base" = basic part, "expand" = extended)
    #[serde(default)]
    component_library_type: String,
    /// Whether this is a preferred/promotional part
    #[serde(default)]
    preferred_component_flag: bool,
}

/// JLCPCB price tier.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbPrice {
    /// Start quantity
    #[serde(default)]
    start_number: i32,
    /// Price in USD
    #[serde(default)]
    product_price: f64,
}

impl From<JlcpcbComponent> for JlcPart {
    fn from(c: JlcpcbComponent) -> Self {
        let price_breaks: Vec<PriceBreak> = c
            .component_prices
            .into_iter()
            .map(|p| PriceBreak {
                qty: p.start_number,
                price: p.product_price,
            })
            .collect();

        let is_basic = c.component_library_type == "base";
        let is_preferred = c.preferred_component_flag;

        // Try to extract package from description if component_specification is empty
        let package = if c.component_specification.is_empty() {
            extract_package_from_description(&c.describe)
        } else {
            c.component_specification
        };

        JlcPart {
            lcsc: c.component_code,
            mpn: c.component_model_en,
            manufacturer: c.component_brand_en,
            category: c.second_sort_name, // second_sort_name is the main category
            subcategory: c.first_sort_name, // first_sort_name is the subcategory
            package,
            description: c.describe,
            stock: c.stock_count,
            price_breaks,
            datasheet: c.datasheet_url.filter(|s| !s.is_empty()),
            basic: is_basic,
            preferred: is_preferred,
            attributes: PartAttributes::default(),
        }
    }
}

/// Extract package size from description (e.g., "0402", "0603", "0805", "SOT-23")
fn extract_package_from_description(desc: &str) -> String {
    use regex::Regex;

    // Match common SMD package sizes
    let patterns = [
        r"\b(0201|0402|0603|0805|1206|1210|2010|2512)\b",
        r"\b(SOT-\d+[A-Z]?)\b",
        r"\b(QFN-\d+)\b",
        r"\b(QFP-\d+)\b",
        r"\b(SOIC-\d+)\b",
        r"\b(SOP-\d+)\b",
        r"\b(TSSOP-\d+)\b",
        r"\b(LQFP-\d+)\b",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(cap) = re.captures(desc) {
                return cap[1].to_string();
            }
        }
    }

    String::new()
}

impl Default for JlcpcbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl JlcpcbClient {
    /// Create a new API client.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Search for parts by keyword (all parts).
    pub fn search(&self, keyword: &str, page: i32, page_size: i32) -> Result<Vec<JlcPart>> {
        self.search_with_filter(keyword, page, page_size, LibraryType::All)
    }

    /// Search with specific library type filter.
    pub fn search_with_filter(
        &self,
        keyword: &str,
        page: i32,
        page_size: i32,
        library_type: LibraryType,
    ) -> Result<Vec<JlcPart>> {
        Ok(self
            .search_page(keyword, page, page_size, library_type)?
            .parts)
    }

    /// Search and return a page with total count (for pagination).
    pub fn search_page(
        &self,
        keyword: &str,
        page: i32,
        page_size: i32,
        library_type: LibraryType,
    ) -> Result<SearchPage> {
        let request_body = JlcpcbSearchRequest::new(keyword, page, page_size, library_type);

        let response = self
            .client
            .post(JLCPCB_SEARCH_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("secretkey", JLCPCB_SECRET_KEY)
            .header("Origin", "https://jlcpcb.com")
            .header("Referer", "https://jlcpcb.com/parts")
            .json(&request_body)
            .send()
            .context("Failed to send search request")?;

        if !response.status().is_success() {
            anyhow::bail!("Search request failed: {}", response.status());
        }

        let search_response: JlcpcbSearchResponse =
            response.json().context("Failed to parse search response")?;

        if search_response.code != 200 {
            anyhow::bail!(
                "JLCPCB API error: {}",
                search_response
                    .message
                    .unwrap_or_else(|| "Unknown error".into())
            );
        }

        let (parts, total) = search_response
            .data
            .and_then(|d| d.component_page_info)
            .map(|p| {
                let total = p.total;
                let parts = p.list.into_iter().map(JlcPart::from).collect();
                (parts, total)
            })
            .unwrap_or_default();

        Ok(SearchPage { parts, total })
    }

    /// Get a single part by LCSC part number.
    pub fn get_part(&self, lcsc: &str) -> Result<Option<JlcPart>> {
        // Search by exact LCSC part number
        let parts = self.search(lcsc, 1, 10)?;
        Ok(parts.into_iter().find(|p| p.lcsc == lcsc))
    }

    /// Get detailed part information including structured attributes.
    pub fn get_part_details(&self, lcsc: &str) -> Result<Option<JlcPart>> {
        // Normalize LCSC code (ensure it starts with C)
        let lcsc_code = if lcsc.starts_with('C') {
            lcsc.to_string()
        } else {
            format!("C{}", lcsc)
        };

        let url = format!("{}?componentCode={}", JLCPCB_DETAIL_URL, lcsc_code);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .context("Failed to send detail request")?;

        if !response.status().is_success() {
            anyhow::bail!("Detail request failed: {}", response.status());
        }

        let detail_response: JlcpcbDetailResponse =
            response.json().context("Failed to parse detail response")?;

        if detail_response.code != 200 {
            return Ok(None);
        }

        Ok(detail_response.data.map(|d| d.into()))
    }

}

/// JLCPCB component detail response.
#[derive(Debug, Deserialize)]
struct JlcpcbDetailResponse {
    code: i32,
    #[serde(default)]
    data: Option<JlcpcbComponentDetail>,
}

/// Detailed component data from the detail endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JlcpcbComponentDetail {
    #[serde(default)]
    component_code: String,
    #[serde(default)]
    component_brand_en: String,
    #[serde(default)]
    component_model_en: String,
    #[serde(default)]
    component_specification_en: String,
    #[serde(default)]
    describe: String,
    #[serde(default)]
    first_sort_name: String,
    #[serde(default)]
    second_sort_name: String,
    #[serde(default)]
    data_manual_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_null_as_empty")]
    attributes: Vec<JlcpcbAttribute>,
}

/// Component attribute from detail response.
#[derive(Debug, Deserialize)]
struct JlcpcbAttribute {
    #[serde(default)]
    attribute_name_en: String,
    #[serde(default)]
    attribute_value_name: String,
}

impl From<JlcpcbComponentDetail> for JlcPart {
    fn from(d: JlcpcbComponentDetail) -> Self {
        let mut attrs = PartAttributes::default();

        // Extract structured attributes
        for attr in &d.attributes {
            match attr.attribute_name_en.as_str() {
                "Capacitance" => attrs.capacitance = Some(attr.attribute_value_name.clone()),
                "Resistance" => attrs.resistance = Some(attr.attribute_value_name.clone()),
                "Inductance" => attrs.inductance = Some(attr.attribute_value_name.clone()),
                "Voltage Rating" | "Rated Voltage" => {
                    attrs.voltage = Some(attr.attribute_value_name.clone())
                }
                "Power" | "Power Rating" => attrs.power = Some(attr.attribute_value_name.clone()),
                "Tolerance" => attrs.tolerance = Some(attr.attribute_value_name.clone()),
                "Temperature Coefficient" | "Dielectric" => {
                    attrs.dielectric = Some(attr.attribute_value_name.clone())
                }
                _ => {}
            }
        }

        JlcPart {
            lcsc: d.component_code,
            mpn: d.component_model_en,
            manufacturer: d.component_brand_en,
            category: d.first_sort_name,
            subcategory: d.second_sort_name,
            package: d.component_specification_en,
            description: d.describe,
            stock: 0, // Not included in detail response
            price_breaks: vec![], // Not included in detail response
            datasheet: d.data_manual_url.filter(|s| !s.is_empty()),
            basic: false, // Not included in detail response
            preferred: false,
            attributes: attrs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires network"]
    fn test_search() {
        let client = JlcpcbClient::new();
        let results = client.search("10k 0402", 1, 10).unwrap();
        assert!(!results.is_empty());
        println!("Found {} results", results.len());
        for part in results.iter().take(3) {
            println!(
                "  {} {} {} ({}) - {}",
                part.lcsc, part.mpn, part.package, part.manufacturer, part.description
            );
        }
    }

    #[test]
    #[ignore = "requires network"]
    fn test_get_part() {
        let client = JlcpcbClient::new();
        let part = client.get_part("C307331").unwrap();
        assert!(part.is_some());
        let part = part.unwrap();
        assert_eq!(part.lcsc, "C307331");
        println!("{:#?}", part);
    }
}
