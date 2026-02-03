//! Generate command - create .zen component files from JLCPCB parts.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::api::{JlcpcbClient, JlcPart};
use crate::generator::{sanitize_mpn, ZenGenerator};
use crate::pins::{extract_pins, ExtractionOptions};

/// Execute the generate command.
pub fn execute(
    lcsc: &str,
    output_dir: Option<PathBuf>,
    name: Option<String>,
    options: &ExtractionOptions,
) -> Result<()> {
    // Normalize LCSC part number
    let lcsc_normalized = if lcsc.starts_with('C') {
        lcsc.to_string()
    } else {
        format!("C{}", lcsc)
    };

    let client = JlcpcbClient::new();

    let mut part = client
        .get_part(&lcsc_normalized)?
        .ok_or_else(|| anyhow::anyhow!("Part {} not found", lcsc_normalized))?;

    // Fetch detailed attributes if not already populated
    if part.attributes.capacitance.is_none()
        && part.attributes.resistance.is_none()
        && part.attributes.inductance.is_none()
    {
        if let Ok(Some(detailed)) = client.get_part_details(&lcsc_normalized) {
            // Merge detailed attributes
            part.attributes = detailed.attributes;
            if part.package.is_empty() && !detailed.package.is_empty() {
                part.package = detailed.package;
            }
            if part.datasheet.is_none() && detailed.datasheet.is_some() {
                part.datasheet = detailed.datasheet;
            }
        }
    }

    // Determine output directory
    let output_dir = output_dir.unwrap_or_else(|| {
        PathBuf::from("components")
            .join("JLCPCB")
            .join(sanitize_mpn(&part.mpn))
    });

    // Create output directory
    fs::create_dir_all(&output_dir).context("Failed to create output directory")?;

    // Determine component name
    let component_name = name.unwrap_or_else(|| sanitize_mpn(&part.mpn));

    // Generate the .zen file
    let generator = ZenGenerator::new();
    let result = generate_zen_content(&generator, &part, &component_name, options)?;

    // Write the .zen file
    let zen_path = output_dir.join(format!("{}.zen", component_name));
    fs::write(&zen_path, &result.zen_content).context("Failed to write .zen file")?;

    // Write symbol file if available
    if let (Some(symbol_content), Some(symbol_filename)) =
        (&result.symbol_content, &result.symbol_filename)
    {
        let symbol_path = output_dir.join(symbol_filename);
        fs::write(&symbol_path, symbol_content).context("Failed to write .kicad_sym file")?;
        println!(
            "{} Created {}",
            "✓".green().bold(),
            symbol_path.display().to_string().cyan()
        );
    }

    // Write footprint file if available
    if let (Some(footprint_content), Some(footprint_filename)) =
        (&result.footprint_content, &result.footprint_filename)
    {
        let footprint_path = output_dir.join(footprint_filename);
        fs::write(&footprint_path, footprint_content).context("Failed to write .kicad_mod file")?;
        println!(
            "{} Created {}",
            "✓".green().bold(),
            footprint_path.display().to_string().cyan()
        );
    }

    // Write pcb.toml if it doesn't exist
    let toml_path = output_dir.join("pcb.toml");
    if !toml_path.exists() {
        fs::write(&toml_path, "").context("Failed to write pcb.toml")?;
    }

    println!(
        "{} Created {}",
        "✓".green().bold(),
        zen_path.display().to_string().cyan()
    );

    // Print part info
    println!("  LCSC: {}", part.lcsc.green());
    println!("  MPN: {}", part.mpn);
    println!("  Manufacturer: {}", part.manufacturer);
    if part.basic {
        println!("  Type: {} (lower assembly fee)", "Basic".green().bold());
    } else if part.preferred {
        println!("  Type: {}", "Preferred".yellow());
    } else {
        println!("  Type: Extended");
    }

    Ok(())
}

/// Result of generating .zen content, may include footprint and symbol data.
struct GenerateResult {
    /// .zen file content
    zen_content: String,
    /// Optional .kicad_mod footprint content
    footprint_content: Option<String>,
    /// Footprint filename (without path)
    footprint_filename: Option<String>,
    /// Optional .kicad_sym symbol content
    symbol_content: Option<String>,
    /// Symbol filename (without path)
    symbol_filename: Option<String>,
}

/// Generate the .zen file content based on part type.
fn generate_zen_content(
    generator: &ZenGenerator,
    part: &JlcPart,
    name: &str,
    options: &ExtractionOptions,
) -> Result<GenerateResult> {
    if part.uses_stdlib_generic() {
        // Use the generic template for passives
        let zen_content = generator.generate_generic(part, name, ("net1", "net2"))?;
        Ok(GenerateResult {
            zen_content,
            footprint_content: None,
            footprint_filename: None,
            symbol_content: None,
            symbol_filename: None,
        })
    } else {
        // Extract pins for non-passive components
        let result = extract_pins(part, options)?;

        // Convert pins to (number, name) tuples for the generator
        let pin_tuples: Vec<(String, String)> = result
            .pins
            .iter()
            .map(|p| (p.number.clone(), p.name.clone()))
            .collect();

        // Generate footprint if we have shape data
        let (footprint_content, footprint_filename) =
            if let Some(footprint) = result.meta.generate_footprint() {
                let filename = format!("{}.kicad_mod", name);
                (Some(footprint), Some(filename))
            } else {
                (None, None)
            };

        // Generate symbol
        let (symbol_content, symbol_filename) =
            if let Some(symbol) = result.meta.generate_symbol(name, &result.pins) {
                let filename = format!("{}.kicad_sym", name);
                (Some(symbol), Some(filename))
            } else {
                (None, None)
            };

        let zen_content = generator.generate_component(
            part,
            name,
            &pin_tuples,
            &result.meta,
            &footprint_filename,
            &symbol_filename,
        )?;

        Ok(GenerateResult {
            zen_content,
            footprint_content,
            footprint_filename,
            symbol_content,
            symbol_filename,
        })
    }
}

/// Generate components for multiple parts at once.
pub fn execute_batch(
    lcsc_parts: &[String],
    output_dir: Option<PathBuf>,
    options: &ExtractionOptions,
) -> Result<()> {
    let client = JlcpcbClient::new();
    let generator = ZenGenerator::new();

    let mut success_count = 0;
    let mut fail_count = 0;

    for lcsc in lcsc_parts {
        let lcsc_normalized = if lcsc.starts_with('C') {
            lcsc.to_string()
        } else {
            format!("C{}", lcsc)
        };

        // Get the part from API
        let part = match client.get_part(&lcsc_normalized) {
            Ok(Some(p)) => p,
            Ok(None) => {
                eprintln!("{} Part {} not found", "✗".red(), lcsc_normalized);
                fail_count += 1;
                continue;
            }
            Err(e) => {
                eprintln!("{} Failed to fetch {}: {}", "✗".red(), lcsc_normalized, e);
                fail_count += 1;
                continue;
            }
        };

        // Determine output directory
        let part_dir = output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("components").join("JLCPCB"))
            .join(sanitize_mpn(&part.mpn));

        // Create output directory
        if let Err(e) = fs::create_dir_all(&part_dir) {
            eprintln!(
                "{} Failed to create directory for {}: {}",
                "✗".red(),
                lcsc_normalized,
                e
            );
            fail_count += 1;
            continue;
        }

        let component_name = sanitize_mpn(&part.mpn);

        // Generate and write
        match generate_zen_content(&generator, &part, &component_name, options) {
            Ok(result) => {
                let zen_path = part_dir.join(format!("{}.zen", component_name));
                if let Err(e) = fs::write(&zen_path, &result.zen_content) {
                    eprintln!(
                        "{} Failed to write {}: {}",
                        "✗".red(),
                        zen_path.display(),
                        e
                    );
                    fail_count += 1;
                    continue;
                }

                // Write symbol file if available
                if let (Some(symbol_content), Some(symbol_filename)) =
                    (&result.symbol_content, &result.symbol_filename)
                {
                    let symbol_path = part_dir.join(symbol_filename);
                    if let Err(e) = fs::write(&symbol_path, symbol_content) {
                        eprintln!(
                            "{} Failed to write {}: {}",
                            "✗".red(),
                            symbol_path.display(),
                            e
                        );
                    }
                }

                // Write footprint file if available
                if let (Some(footprint_content), Some(footprint_filename)) =
                    (&result.footprint_content, &result.footprint_filename)
                {
                    let footprint_path = part_dir.join(footprint_filename);
                    if let Err(e) = fs::write(&footprint_path, footprint_content) {
                        eprintln!(
                            "{} Failed to write {}: {}",
                            "✗".red(),
                            footprint_path.display(),
                            e
                        );
                    }
                }

                // Write pcb.toml
                let toml_path = part_dir.join("pcb.toml");
                if !toml_path.exists() {
                    let _ = fs::write(&toml_path, "");
                }

                println!(
                    "{} {} → {}",
                    "✓".green(),
                    lcsc_normalized,
                    zen_path.display().to_string().cyan()
                );
                success_count += 1;
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to generate for {}: {}",
                    "✗".red(),
                    lcsc_normalized,
                    e
                );
                fail_count += 1;
            }
        }
    }

    println!(
        "\n{} Generated {} components, {} failed",
        if fail_count == 0 {
            "✓".green().bold()
        } else {
            "!".yellow().bold()
        },
        success_count,
        fail_count
    );

    Ok(())
}
