//! BOM commands - check availability and export for JLCPCB assembly.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;
use regex::Regex;
use serde::Deserialize;

use crate::api::{JlcpcbClient, JlcPart};

/// BOM entry extracted from a .zen file or BOM JSON.
#[derive(Debug, Clone)]
pub struct BomEntry {
    /// Component designators (e.g., "C1", "C2")
    pub designators: Vec<String>,
    /// LCSC part number if specified
    pub lcsc: Option<String>,
    /// Manufacturer part number
    pub mpn: Option<String>,
    /// Quantity
    pub quantity: usize,
    /// Component value (for passives)
    pub value: Option<String>,
    /// Package/footprint
    pub package: Option<String>,
}

/// BOM check result for a single line.
#[derive(Debug)]
pub struct BomCheckResult {
    pub entry: BomEntry,
    pub part: Option<JlcPart>,
    pub status: BomStatus,
}

/// Status of a BOM line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomStatus {
    /// Part found with sufficient stock
    Ok,
    /// Part found but limited stock
    Limited,
    /// Part not found or insufficient stock
    Missing,
    /// Part found but not a basic part
    Extended,
}

impl BomStatus {
    fn symbol(&self) -> colored::ColoredString {
        match self {
            BomStatus::Ok => "✓".green(),
            BomStatus::Limited => "!".yellow(),
            BomStatus::Missing => "✗".red(),
            BomStatus::Extended => "~".blue(),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            BomStatus::Ok => "OK",
            BomStatus::Limited => "Limited",
            BomStatus::Missing => "Missing",
            BomStatus::Extended => "Extended",
        }
    }
}

/// Execute the BOM check command.
pub fn execute_check(bom_path: &PathBuf, quantity: i32) -> Result<()> {
    let entries = load_bom(bom_path)?;

    if entries.is_empty() {
        println!("{} No BOM entries found", "✗".red());
        return Ok(());
    }

    let client = JlcpcbClient::new();

    let mut results = Vec::new();

    for entry in entries {
        let required_qty = entry.quantity as i32 * quantity;

        // Try to find the part
        let (part, status) = if let Some(ref lcsc) = entry.lcsc {
            // Look up by LCSC number
            let part = client.get_part(lcsc).ok().flatten();

            if let Some(ref p) = part {
                let status = if p.stock >= required_qty as i64 {
                    if p.basic {
                        BomStatus::Ok
                    } else {
                        BomStatus::Extended
                    }
                } else if p.stock > 0 {
                    BomStatus::Limited
                } else {
                    BomStatus::Missing
                };
                (part, status)
            } else {
                (None, BomStatus::Missing)
            }
        } else if let Some(ref mpn) = entry.mpn {
            // Search by MPN
            let parts = client.search(mpn, 1, 5).unwrap_or_default();
            if let Some(p) = parts.into_iter().find(|p| p.basic) {
                let status = if p.stock >= required_qty as i64 {
                    BomStatus::Ok
                } else if p.stock > 0 {
                    BomStatus::Limited
                } else {
                    BomStatus::Missing
                };
                (Some(p), status)
            } else {
                (None, BomStatus::Missing)
            }
        } else {
            (None, BomStatus::Missing)
        };

        results.push(BomCheckResult {
            entry,
            part,
            status,
        });
    }

    // Print results
    println!(
        "\n{:<10} {:<15} {:<10} {:>10} {:<8}",
        "Status".bold(),
        "Designators".bold(),
        "LCSC".bold(),
        "Stock".bold(),
        "Basic?".bold()
    );
    println!("{}", "-".repeat(60));

    let mut ok_count = 0;
    let mut limited_count = 0;
    let mut missing_count = 0;
    let mut extended_count = 0;

    for result in &results {
        let designators = if result.entry.designators.len() > 3 {
            format!(
                "{}-{}",
                result.entry.designators.first().unwrap(),
                result.entry.designators.last().unwrap()
            )
        } else {
            result.entry.designators.join(",")
        };

        let (lcsc, stock, basic) = if let Some(ref p) = result.part {
            (
                p.lcsc.clone(),
                format_stock(p.stock),
                if p.basic { "Yes" } else { "No" },
            )
        } else {
            ("—".to_string(), "—".to_string(), "—")
        };

        println!(
            "{} {:<8} {:<15} {:<10} {:>10} {:<8}",
            result.status.symbol(),
            result.status.label(),
            designators,
            lcsc,
            stock,
            basic
        );

        match result.status {
            BomStatus::Ok => ok_count += 1,
            BomStatus::Limited => limited_count += 1,
            BomStatus::Missing => missing_count += 1,
            BomStatus::Extended => extended_count += 1,
        }
    }

    // Print summary
    println!();
    println!(
        "{} OK: {}, Limited: {}, Extended: {}, Missing: {}",
        "Summary:".bold(),
        ok_count.to_string().green(),
        limited_count.to_string().yellow(),
        extended_count.to_string().blue(),
        missing_count.to_string().red()
    );

    if missing_count > 0 {
        println!(
            "\n{} {} parts missing - search for alternatives with `pcb jlcpcb search`",
            "!".yellow().bold(),
            missing_count
        );
    }

    Ok(())
}

/// Execute the BOM export command (JLCPCB CSV format).
pub fn execute_export(bom_path: &PathBuf, output: &PathBuf) -> Result<()> {
    let entries = load_bom(bom_path)?;

    if entries.is_empty() {
        println!("{} No BOM entries found", "✗".red());
        return Ok(());
    }

    let client = JlcpcbClient::new();

    // JLCPCB BOM format: Comment,Designator,Footprint,LCSC Part #
    let mut output_file = fs::File::create(output).context("Failed to create output file")?;

    writeln!(output_file, "Comment,Designator,Footprint,LCSC Part #")?;

    let mut exported_count = 0;
    let mut missing_count = 0;

    for entry in entries {
        let designators = entry.designators.join(",");
        let footprint = entry.package.clone().unwrap_or_default();

        // Try to get LCSC number
        let lcsc = if let Some(ref l) = entry.lcsc {
            Some(l.clone())
        } else if let Some(ref mpn) = entry.mpn {
            // Search for LCSC number
            let parts = client.search(mpn, 1, 5).unwrap_or_default();
            parts.into_iter().find(|p| p.basic).map(|p| p.lcsc)
        } else {
            None
        };

        if let Some(lcsc) = lcsc {
            // Get part info for comment
            let comment = if let Some(p) = client.get_part(&lcsc).ok().flatten() {
                format!("{} {}", p.mpn, p.description)
            } else {
                entry.mpn.clone().unwrap_or_else(|| entry.value.clone().unwrap_or_default())
            };

            writeln!(
                output_file,
                "\"{}\",\"{}\",\"{}\",\"{}\"",
                comment.replace('"', "\"\""),
                designators,
                footprint,
                lcsc
            )?;
            exported_count += 1;
        } else {
            // Write without LCSC number (will need manual entry)
            let comment = entry.mpn.clone().unwrap_or_else(|| entry.value.clone().unwrap_or_default());
            writeln!(
                output_file,
                "\"{}\",\"{}\",\"{}\",\"\"",
                comment.replace('"', "\"\""),
                designators,
                footprint
            )?;
            missing_count += 1;
        }
    }

    println!(
        "{} Exported {} lines to {}",
        "✓".green().bold(),
        exported_count,
        output.display().to_string().cyan()
    );

    if missing_count > 0 {
        println!(
            "{} {} lines missing LCSC part numbers",
            "!".yellow(),
            missing_count
        );
    }

    Ok(())
}

/// Load BOM entries from a file (JSON or .zen).
fn load_bom(path: &PathBuf) -> Result<Vec<BomEntry>> {
    let content = fs::read_to_string(path).context("Failed to read BOM file")?;

    if path.extension().is_some_and(|e| e == "json") {
        load_bom_json(&content)
    } else {
        // Assume it's a .zen file or directory - try to extract LCSC properties
        load_bom_from_zen(&content, path)
    }
}

/// Load BOM from JSON format.
fn load_bom_json(content: &str) -> Result<Vec<BomEntry>> {
    #[derive(Deserialize)]
    struct JsonBomEntry {
        designators: Vec<String>,
        #[serde(default)]
        lcsc: Option<String>,
        #[serde(default)]
        mpn: Option<String>,
        #[serde(default)]
        value: Option<String>,
        #[serde(default)]
        package: Option<String>,
    }

    let entries: Vec<JsonBomEntry> =
        serde_json::from_str(content).context("Failed to parse BOM JSON")?;

    Ok(entries
        .into_iter()
        .map(|e| BomEntry {
            quantity: e.designators.len(),
            designators: e.designators,
            lcsc: e.lcsc,
            mpn: e.mpn,
            value: e.value,
            package: e.package,
        })
        .collect())
}

/// Extract BOM entries from .zen file content.
fn load_bom_from_zen(content: &str, _path: &PathBuf) -> Result<Vec<BomEntry>> {
    let mut entries = Vec::new();

    // Look for LCSC Part properties in the file
    // Pattern: "LCSC Part": "C307331"
    let lcsc_pattern = Regex::new(r#""LCSC Part":\s*"(C\d+)""#)?;
    let name_pattern = Regex::new(r#"name\s*=\s*"([^"]+)""#)?;

    // Group entries by LCSC part
    let mut lcsc_to_designators: HashMap<String, Vec<String>> = HashMap::new();

    // This is a simplified parser - in practice you'd want to properly parse the .zen file
    for cap in lcsc_pattern.captures_iter(content) {
        let lcsc = cap[1].to_string();

        // Try to find the associated component name
        // This is a hack - proper implementation would parse the AST
        let designator = name_pattern
            .captures_iter(content)
            .next()
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "U1".to_string());

        lcsc_to_designators
            .entry(lcsc)
            .or_default()
            .push(designator);
    }

    for (lcsc, designators) in lcsc_to_designators {
        entries.push(BomEntry {
            quantity: designators.len(),
            designators,
            lcsc: Some(lcsc),
            mpn: None,
            value: None,
            package: None,
        });
    }

    Ok(entries)
}

/// Format stock number for display.
fn format_stock(stock: i64) -> String {
    if stock >= 1_000_000 {
        format!("{}M+", stock / 1_000_000)
    } else if stock >= 1_000 {
        format!("{}K", stock / 1_000)
    } else {
        stock.to_string()
    }
}
