//! BOM commands - check availability and export for JLCPCB assembly.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use tabled::{
    settings::{style::Style, Alignment, Modify},
    Table, Tabled,
};

use crate::api::{JlcpcbClient, JlcPart};

/// BOM entry extracted from a .zen file or BOM JSON.
#[derive(Debug, Clone)]
pub struct BomEntry {
    /// Component designators (e.g., "C1", "C2")
    pub designators: Vec<String>,
    /// LCSC part number candidates (e.g., ["C237493", "C21721099"])
    pub lcsc_candidates: Vec<String>,
    /// Manufacturer part number
    pub mpn: Option<String>,
    /// Quantity
    pub quantity: usize,
    /// Component value (for passives)
    pub value: Option<String>,
    /// Package/footprint
    pub package: Option<String>,
    /// Component is marked Do Not Place
    pub dnp: bool,
}

/// BOM check result for a single line.
#[derive(Debug)]
pub struct BomCheckResult {
    pub entry: BomEntry,
    pub part: Option<JlcPart>,
    pub status: BomStatus,
}

/// Status of a BOM line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BomStatus {
    /// Part found with sufficient stock
    Ok,
    /// Part found but limited stock
    Limited,
    /// Part not found or insufficient stock
    Missing,
    /// Part found but not a basic part
    Extended,
    /// Component marked Do Not Place
    Dnp,
}

impl BomStatus {
    fn symbol(&self) -> colored::ColoredString {
        match self {
            BomStatus::Ok => "■".green(),
            BomStatus::Limited => "■".yellow(),
            BomStatus::Missing => "■".red(),
            BomStatus::Extended => "■".blue(),
            BomStatus::Dnp => "■".dimmed(),
        }
    }
}

/// Table row for BOM check results.
#[derive(Tabled)]
struct BomCheckRow {
    #[tabled(rename = "")]
    indicator: String,
    #[tabled(rename = "Designators")]
    designators: String,
    #[tabled(rename = "LCSC")]
    lcsc: String,
    #[tabled(rename = "Stock")]
    stock: String,
    #[tabled(rename = "Price@100")]
    price: String,
}

/// Resolve the best LCSC part from a list of candidates.
///
/// Queries each candidate and returns the best match using priority:
/// basic > preferred > extended, then highest stock within each tier.
fn resolve_best_lcsc(candidates: &[String], client: &JlcpcbClient) -> Option<(String, JlcPart)> {
    let mut parts: Vec<(String, JlcPart)> = candidates
        .iter()
        .filter_map(|lcsc| {
            client
                .get_part(lcsc)
                .ok()
                .flatten()
                .map(|p| (lcsc.clone(), p))
        })
        .collect();

    // Sort: basic first, then preferred, then extended; within tier sort by stock desc
    parts.sort_by(|(_, a), (_, b)| {
        let tier = |p: &JlcPart| {
            if p.basic {
                0
            } else if p.preferred {
                1
            } else {
                2
            }
        };
        tier(a).cmp(&tier(b)).then(b.stock.cmp(&a.stock))
    });

    parts.into_iter().next()
}

/// JSON output for a BOM check result.
#[derive(Serialize)]
struct BomCheckJson {
    designators: Vec<String>,
    status: BomStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    lcsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mpn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stock: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_at_100: Option<f64>,
    dnp: bool,
}

/// JSON output for a BOM export line.
#[derive(Serialize)]
struct BomExportJson {
    comment: String,
    designators: Vec<String>,
    footprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    lcsc: Option<String>,
}

/// Execute the BOM check command.
pub fn execute_check(bom_path: &PathBuf, quantity: i32, include_dnp: bool, json: bool, refresh: bool) -> Result<()> {
    let entries = load_bom(bom_path)?;

    if entries.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("{} No BOM entries found", "✗".red());
        }
        return Ok(());
    }

    let client = JlcpcbClient::new().with_cache(!refresh);

    let mut results = Vec::new();

    for entry in entries {
        // DNP entries get shown in the table but skip API lookups
        if entry.dnp && !include_dnp {
            results.push(BomCheckResult {
                entry,
                part: None,
                status: BomStatus::Dnp,
            });
            continue;
        }

        let required_qty = entry.quantity as i32 * quantity;

        // Try to find the part
        let (part, status) = if !entry.lcsc_candidates.is_empty() {
            // Try resolving from LCSC candidates
            if let Some((_lcsc, p)) = resolve_best_lcsc(&entry.lcsc_candidates, &client) {
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
                (Some(p), status)
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

    if json {
        let json_results: Vec<BomCheckJson> = results
            .into_iter()
            .map(|r| BomCheckJson {
                designators: r.entry.designators,
                status: r.status,
                lcsc: r.part.as_ref().map(|p| p.lcsc.clone()),
                mpn: r.entry.mpn,
                value: r.entry.value,
                package: r.entry.package,
                stock: r.part.as_ref().map(|p| p.stock),
                price_at_100: r.part.as_ref().and_then(|p| p.price_at_qty(100)),
                dnp: r.entry.dnp,
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_results)?);
        return Ok(());
    }

    // Build table rows and tally status counts
    let mut ok_count = 0;
    let mut limited_count = 0;
    let mut missing_count = 0;
    let mut extended_count = 0;
    let mut dnp_count = 0;

    let rows: Vec<BomCheckRow> = results
        .iter()
        .map(|result| {
            match result.status {
                BomStatus::Ok => ok_count += 1,
                BomStatus::Limited => limited_count += 1,
                BomStatus::Missing => missing_count += 1,
                BomStatus::Extended => extended_count += 1,
                BomStatus::Dnp => dnp_count += 1,
            }

            let designators = if result.entry.designators.len() > 3 {
                format!(
                    "{}-{}",
                    result.entry.designators.first().unwrap(),
                    result.entry.designators.last().unwrap()
                )
            } else {
                result.entry.designators.join(",")
            };

            let (lcsc, stock, price) = if let Some(ref p) = result.part {
                (
                    p.lcsc.clone(),
                    format_stock(p.stock),
                    p.price_at_qty(100)
                        .map(|v| format!("${:.4}", v))
                        .unwrap_or_else(|| "—".to_string()),
                )
            } else {
                ("—".to_string(), "—".to_string(), "—".to_string())
            };

            BomCheckRow {
                indicator: result.status.symbol().to_string(),
                designators,
                lcsc,
                stock,
                price,
            }
        })
        .collect();

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Modify::new(tabled::settings::object::Columns::new(3..=4)).with(Alignment::right()))
        .to_string();

    println!("\n{}", table);
    println!(
        "{} Ok  {} Limited  {} Extended  {} Missing  {} DNP",
        "■".green(),
        "■".yellow(),
        "■".blue(),
        "■".red(),
        "■".dimmed()
    );

    // Print summary
    println!();
    println!(
        "{} OK: {}, Limited: {}, Extended: {}, Missing: {}, DNP: {}",
        "Summary:".bold(),
        ok_count.to_string().green(),
        limited_count.to_string().yellow(),
        extended_count.to_string().blue(),
        missing_count.to_string().red(),
        dnp_count.to_string().dimmed()
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
pub fn execute_export(bom_path: &PathBuf, output: &PathBuf, include_dnp: bool, json: bool, refresh: bool) -> Result<()> {
    let all_entries = load_bom(bom_path)?;

    if all_entries.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("{} No BOM entries found", "✗".red());
        }
        return Ok(());
    }

    let (entries, dnp_entries): (Vec<_>, Vec<_>) = all_entries
        .into_iter()
        .partition(|e| include_dnp || !e.dnp);

    if entries.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("{} No BOM entries found (all components are DNP)", "✗".red());
            if !dnp_entries.is_empty() {
                let dnp_designators: Vec<String> = dnp_entries
                    .iter()
                    .flat_map(|e| &e.designators)
                    .cloned()
                    .collect();
                println!(
                    "\nSkipped {} DNP components ({})",
                    dnp_designators.len(),
                    dnp_designators.join(", ")
                );
            }
        }
        return Ok(());
    }

    let client = JlcpcbClient::new().with_cache(!refresh);

    let mut json_rows: Vec<BomExportJson> = Vec::new();
    let mut output_file = if !json {
        let f = fs::File::create(output).context("Failed to create output file")?;
        Some(f)
    } else {
        None
    };

    if let Some(ref mut f) = output_file {
        writeln!(f, "Comment,Designator,Footprint,LCSC Part #")?;
    }

    let mut exported_count = 0;
    let mut missing_count = 0;

    for entry in entries {
        let designators_str = entry.designators.join(",");
        let footprint = entry.package.clone().unwrap_or_default();

        // Try to get LCSC number
        let resolved = if !entry.lcsc_candidates.is_empty() {
            resolve_best_lcsc(&entry.lcsc_candidates, &client)
        } else if let Some(ref mpn) = entry.mpn {
            // Search for LCSC number by MPN
            let parts = client.search(mpn, 1, 5).unwrap_or_default();
            parts
                .into_iter()
                .find(|p| p.basic)
                .map(|p| (p.lcsc.clone(), p))
        } else {
            None
        };

        if let Some((lcsc, part)) = resolved {
            let comment = format!("{} {}", part.mpn, part.description);

            if json {
                json_rows.push(BomExportJson {
                    comment,
                    designators: entry.designators,
                    footprint,
                    lcsc: Some(lcsc),
                });
            } else {
                writeln!(
                    output_file.as_mut().unwrap(),
                    "\"{}\",\"{}\",\"{}\",\"{}\"",
                    comment.replace('"', "\"\""),
                    designators_str,
                    footprint,
                    lcsc
                )?;
            }
            exported_count += 1;
        } else {
            let comment = entry
                .mpn
                .clone()
                .unwrap_or_else(|| entry.value.clone().unwrap_or_default());

            if json {
                json_rows.push(BomExportJson {
                    comment,
                    designators: entry.designators,
                    footprint,
                    lcsc: None,
                });
            } else {
                writeln!(
                    output_file.as_mut().unwrap(),
                    "\"{}\",\"{}\",\"{}\",\"\"",
                    comment.replace('"', "\"\""),
                    designators_str,
                    footprint
                )?;
            }
            missing_count += 1;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&json_rows)?);
        return Ok(());
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

    if !dnp_entries.is_empty() {
        let dnp_designators: Vec<String> = dnp_entries
            .iter()
            .flat_map(|e| &e.designators)
            .cloned()
            .collect();
        println!(
            "\nSkipped {} DNP components ({})",
            dnp_designators.len(),
            dnp_designators.join(", ")
        );
    }

    Ok(())
}

/// Load BOM entries from a file (JSON or .zen).
fn load_bom(path: &PathBuf) -> Result<Vec<BomEntry>> {
    if path.extension().is_some_and(|e| e == "json") {
        let content = fs::read_to_string(path).context("Failed to read BOM file")?;
        load_bom_json(&content)
    } else {
        // Assume it's a .zen file - shell out to `pcb bom` to get JSON
        load_bom_from_zen(path)
    }
}

// ── JSON deserialization structs ──────────────────────────────────────────────

/// Existing grouped BOM format (plural `designators`).
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
    #[serde(default)]
    dnp: Option<bool>,
}

/// Per-designator BOM format from `pcb bom -f json` (singular `designator`).
#[derive(Deserialize)]
struct PcbBomEntry {
    designator: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    mpn: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    availability: Option<PcbBomAvailability>,
    #[serde(default)]
    dnp: bool,
}

#[derive(Deserialize)]
struct PcbBomAvailability {
    #[serde(default)]
    offers: Vec<PcbBomOffer>,
}

#[derive(Deserialize)]
struct PcbBomOffer {
    #[serde(default)]
    distributor: Option<String>,
    #[serde(default)]
    part_id: Option<String>,
    #[serde(default)]
    stock: Option<i64>,
}

/// Load BOM from JSON format.
///
/// Tries the flat `pcb bom -f json` format first (singular `designator`),
/// falls back to the grouped format (plural `designators`).
fn load_bom_json(content: &str) -> Result<Vec<BomEntry>> {
    // Try flat pcb-bom format first
    if let Ok(flat_entries) = serde_json::from_str::<Vec<PcbBomEntry>>(content) {
        return Ok(group_pcb_bom_entries(flat_entries));
    }

    // Fall back to grouped format
    let entries: Vec<JsonBomEntry> =
        serde_json::from_str(content).context("Failed to parse BOM JSON")?;

    Ok(entries
        .into_iter()
        .map(|e| BomEntry {
            quantity: e.designators.len(),
            designators: e.designators,
            lcsc_candidates: e.lcsc.into_iter().collect(),
            mpn: e.mpn,
            value: e.value,
            package: e.package,
            dnp: e.dnp.unwrap_or(false),
        })
        .collect())
}

/// Group flat per-designator entries into merged BomEntry values.
///
/// Entries are grouped by MPN (same MPN = same physical part).
/// For entries without MPN, fall back to (value, package) as key.
fn group_pcb_bom_entries(entries: Vec<PcbBomEntry>) -> Vec<BomEntry> {
    // Key: either MPN or (value, package) tuple serialized
    let mut groups: HashMap<String, (Vec<String>, Option<String>, Option<String>, Option<String>, Option<String>, Vec<PcbBomOffer>, bool)> =
        HashMap::new();
    // Preserve insertion order
    let mut key_order: Vec<String> = Vec::new();

    for entry in entries {
        let key = if let Some(ref mpn) = entry.mpn {
            format!("mpn:{}", mpn)
        } else {
            format!(
                "vp:{}:{}",
                entry.value.as_deref().unwrap_or(""),
                entry.package.as_deref().unwrap_or("")
            )
        };

        let group = groups.entry(key.clone()).or_insert_with(|| {
            key_order.push(key.clone());
            (Vec::new(), entry.mpn.clone(), entry.value.clone(), entry.package.clone(), entry.description.clone(), Vec::new(), true)
        });

        // A group is DNP only if all entries in the group are DNP
        group.6 = group.6 && entry.dnp;

        let user_name = entry.path.as_deref()
            .and_then(|p| p.split('.').next())
            .filter(|n| !n.is_empty())
            .unwrap_or(&entry.designator);
        group.0.push(user_name.to_string());

        if let Some(availability) = entry.availability {
            group.5.extend(availability.offers);
        }
    }

    key_order
        .into_iter()
        .filter_map(|key| {
            let (designators, mpn, value, package, _description, offers, dnp) = groups.remove(&key)?;
            let lcsc_candidates = extract_lcsc_candidates(&offers);
            Some(BomEntry {
                quantity: designators.len(),
                designators,
                lcsc_candidates,
                mpn,
                value,
                package,
                dnp,
            })
        })
        .collect()
}

/// Extract and deduplicate LCSC part_id values from offers.
///
/// Normalizes bare numbers like "237493" to "C237493", filters out
/// zero-stock offers, and sorts by stock descending.
fn extract_lcsc_candidates(offers: &[PcbBomOffer]) -> Vec<String> {
    let mut candidates: Vec<(String, i64)> = offers
        .iter()
        .filter(|o| {
            o.distributor
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case("lcsc") || d.eq_ignore_ascii_case("jlcpcb"))
        })
        .filter_map(|o| {
            let part_id = o.part_id.as_deref()?.trim();
            if part_id.is_empty() {
                return None;
            }
            let stock = o.stock.unwrap_or(0);
            if stock <= 0 {
                return None;
            }
            // Normalize: ensure it starts with 'C'
            let normalized = if part_id.starts_with('C') || part_id.starts_with('c') {
                format!("C{}", &part_id[1..])
            } else if part_id.chars().all(|c| c.is_ascii_digit()) {
                format!("C{}", part_id)
            } else {
                part_id.to_string()
            };
            Some((normalized, stock))
        })
        .collect();

    // Sort by stock descending
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    // Deduplicate preserving order
    let mut seen = Vec::new();
    for (id, _) in candidates {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen
}

/// Load BOM from a .zen file by shelling out to `pcb bom -f json`.
fn load_bom_from_zen(path: &PathBuf) -> Result<Vec<BomEntry>> {
    let output = Command::new("pcb")
        .args(["bom", "-f", "json"])
        .arg(path)
        .output()
        .context("Failed to run `pcb bom`. Is the `pcb` CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`pcb bom -f json` failed (exit {}):\n{}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 in pcb bom output")?;

    // Try flat pcb-bom format first so we can build the layout→zen mapping
    // before grouping (grouping loses access to per-entry designator/path).
    if let Ok(flat_entries) = serde_json::from_str::<Vec<PcbBomEntry>>(&stdout) {
        let layout_to_zen = build_layout_to_zen_map(&flat_entries);
        let mut entries = group_pcb_bom_entries(flat_entries);

        let dnp_refs = read_layout_dnp(path, &layout_to_zen);
        if !dnp_refs.is_empty() {
            apply_layout_dnp(&mut entries, &dnp_refs);
        }

        return Ok(entries);
    }

    // Fallback: grouped format (no per-entry mapping available)
    let mut entries = load_bom_json(&stdout)?;

    let dnp_refs = read_layout_dnp(path, &HashMap::new());
    if !dnp_refs.is_empty() {
        apply_layout_dnp(&mut entries, &dnp_refs);
    }

    Ok(entries)
}

/// Build a mapping from layout designators to zen names.
///
/// For each flat BOM entry, the `designator` field is the layout reference
/// (auto-assigned by pcb, e.g. "J1") and the `path` field contains the
/// zen name as its first dot-separated component (e.g. "J3.SpeakerPads" → "J3").
fn build_layout_to_zen_map(entries: &[PcbBomEntry]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in entries {
        let zen_name = entry
            .path
            .as_deref()
            .and_then(|p| p.split('.').next())
            .filter(|n| !n.is_empty())
            .unwrap_or(&entry.designator);
        map.insert(entry.designator.clone(), zen_name.to_string());
    }
    map
}

/// Read DNP reference designators from the `.kicad_pcb` layout file associated
/// with a `.zen` project, translated to zen names via the provided mapping.
///
/// The `.zen` file contains a `layout_path` field pointing to a directory that
/// holds `layout.kicad_pcb`. Footprints in that file with `(attr ... dnp)` are
/// considered DNP. Layout refs are translated to zen names using `layout_to_zen`;
/// unmapped refs are included as-is (handles cases where zen name equals layout
/// ref). Returns an empty set on any failure (missing file, parse error, etc.).
fn read_layout_dnp(zen_path: &Path, layout_to_zen: &HashMap<String, String>) -> HashSet<String> {
    let zen_content = match fs::read_to_string(zen_path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };

    // Extract layout_path from the .zen file
    let layout_path_re = regex::Regex::new(r#"layout_path\s*=\s*"([^"]+)""#).unwrap();
    let layout_rel = match layout_path_re.captures(&zen_content) {
        Some(caps) => caps[1].to_string(),
        None => return HashSet::new(),
    };

    // Resolve to absolute: zen_dir / layout_path / "layout.kicad_pcb"
    let zen_dir = match zen_path.parent() {
        Some(d) => d,
        None => return HashSet::new(),
    };
    let kicad_path = zen_dir.join(&layout_rel).join("layout.kicad_pcb");

    let content = match fs::read_to_string(&kicad_path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };

    let layout_refs = parse_kicad_dnp(&content);

    // Translate layout refs to zen names
    layout_refs
        .into_iter()
        .map(|r| layout_to_zen.get(&r).cloned().unwrap_or(r))
        .collect()
}

/// Parse a `.kicad_pcb` file and return the set of reference designators that
/// have the `dnp` attribute (i.e. `(attr ... dnp)` inside a `(footprint ...)` block).
fn parse_kicad_dnp(content: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    let ref_re = regex::Regex::new(r#"\(property\s+"Reference"\s+"([^"]+)""#).unwrap();

    // We scan for top-level `(footprint ` blocks (depth 1) by tracking parens.
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut depth: i32 = 0;

    while i < len {
        match bytes[i] {
            b'(' => {
                depth += 1;
                // Check if this opens a footprint block at depth 1
                // (depth just became 1 means we're at top-level of the file,
                //  but footprint blocks are children of the top-level kicad_pcb,
                //  so they start at depth 2)
                if depth == 2 {
                    let rest = &content[i..];
                    if rest.starts_with("(footprint ") {
                        // Find the matching closing paren for this footprint block
                        let block_start = i;
                        let mut fp_depth = 1i32;
                        let mut j = i + 1;
                        while j < len && fp_depth > 0 {
                            match bytes[j] {
                                b'(' => fp_depth += 1,
                                b')' => fp_depth -= 1,
                                _ => {}
                            }
                            j += 1;
                        }
                        let block = &content[block_start..j];

                        // Check for DNP attribute: (attr ... dnp)
                        let has_dnp = block.contains("(attr dnp)")
                            || block.contains("(attr smd dnp)")
                            || block.contains("(attr through_hole dnp)");

                        if has_dnp {
                            if let Some(caps) = ref_re.captures(block) {
                                result.insert(caps[1].to_string());
                            }
                        }

                        // Skip past this block
                        depth = 1; // back to the kicad_pcb level
                        i = j;
                        continue;
                    }
                }
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    result
}

/// Apply layout DNP status to BOM entries. If any designator in an entry
/// appears in `dnp_refs`, the entry is marked as DNP.
fn apply_layout_dnp(entries: &mut [BomEntry], dnp_refs: &HashSet<String>) {
    for entry in entries.iter_mut() {
        if entry.designators.iter().any(|d| dnp_refs.contains(d)) {
            entry.dnp = true;
        }
    }
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
