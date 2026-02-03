//! Search command - find parts in JLCPCB parts library.

use anyhow::Result;
use colored::Colorize;
use tabled::{
    settings::{style::Style, Alignment, Modify},
    Table, Tabled,
};

use crate::api::{JlcpcbClient, JlcPart, LibraryType};

/// Output format for search results.
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Table row for search results.
#[derive(Tabled)]
struct PartRow {
    #[tabled(rename = "")]
    indicator: String,
    #[tabled(rename = "LCSC")]
    lcsc: String,
    #[tabled(rename = "MPN")]
    mpn: String,
    #[tabled(rename = "Package")]
    package: String,
    #[tabled(rename = "Value")]
    value: String,
    #[tabled(rename = "Stock")]
    stock: String,
    #[tabled(rename = "Price@100")]
    price: String,
}

/// Execute the search command.
pub fn execute(
    query: &str,
    format: OutputFormat,
    library_type: LibraryType,
    limit: usize,
    page: i32,
) -> Result<()> {
    let client = JlcpcbClient::new();
    let result = client.search_page(query, page, limit as i32, library_type)?;
    let refs: Vec<&JlcPart> = result.parts.iter().collect();

    match format {
        OutputFormat::Human => print_human(&refs, query, page, result.total, limit),
        OutputFormat::Json => print_json(&refs)?,
    }

    Ok(())
}

fn print_human(results: &[&JlcPart], query: &str, page: i32, total: i64, page_size: usize) {
    if results.is_empty() {
        println!(
            "{} No results found for '{}'",
            "✗".red().bold(),
            query.cyan()
        );
        return;
    }

    // Build table rows
    let rows: Vec<PartRow> = results
        .iter()
        .map(|part| {
            let indicator = if part.basic {
                "■".green().to_string()
            } else if part.preferred {
                "■".yellow().to_string()
            } else {
                " ".to_string()
            };

            PartRow {
                indicator,
                lcsc: part.lcsc.clone(),
                mpn: truncate(&part.mpn, 24),
                package: part.package.clone(),
                value: extract_display_value(part),
                stock: format_stock(part.stock),
                price: part
                    .price_at_qty(100)
                    .map(|p| format!("${:.4}", p))
                    .unwrap_or_else(|| "—".to_string()),
            }
        })
        .collect();

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Modify::new(tabled::settings::object::Columns::new(4..=5)).with(Alignment::right()))
        .to_string();

    println!("{}", table);

    // Footer with pagination and legend
    let total_pages = (total as usize + page_size - 1) / page_size;
    println!(
        "Page {}/{} ({} total)  {} Basic  {} Preferred",
        page,
        total_pages,
        total,
        "■".green(),
        "■".yellow()
    );
}

fn print_json(results: &[&JlcPart]) -> Result<()> {
    let json = serde_json::to_string_pretty(results)?;
    println!("{}", json);
    Ok(())
}

/// Extract a display value from a part (resistance, capacitance, etc.).
fn extract_display_value(part: &JlcPart) -> String {
    if let Some(ref r) = part.attributes.resistance {
        return r.clone();
    }
    if let Some(ref c) = part.attributes.capacitance {
        return c.clone();
    }
    if let Some(ref i) = part.attributes.inductance {
        return i.clone();
    }
    // Try to extract from description
    extract_value_from_desc(&part.description).unwrap_or_else(|| "—".to_string())
}

/// Try to extract a value from a part description.
fn extract_value_from_desc(desc: &str) -> Option<String> {
    use regex::Regex;

    // Match common value patterns
    let patterns = [
        r"(\d+(?:\.\d+)?[kKmMuUnNpP]?[ΩFH])",
        r"(\d+(?:\.\d+)?[kKmMrR])\b",
    ];

    for pattern in patterns {
        if let Some(caps) = Regex::new(pattern).ok()?.captures(desc) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    None
}

/// Format stock number with commas.
fn format_stock(stock: i64) -> String {
    if stock >= 1_000_000 {
        format!("{}M+", stock / 1_000_000)
    } else if stock >= 1_000 {
        format!("{}K", stock / 1_000)
    } else {
        stock.to_string()
    }
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
