//! pcb-jlcpcb - JLCPCB parts library integration for pcb.
//!
//! This is a standalone CLI tool that integrates with the pcb workflow
//! via the plugin mechanism (executables named `pcb-<command>` become
//! available as `pcb <command>`).

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod api;
mod commands;
mod easyeda;
mod generator;
mod pins;

#[derive(Parser)]
#[command(name = "pcb-jlcpcb")]
#[command(author, version, about = "JLCPCB parts library integration for pcb")]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for parts in the JLCPCB parts library
    Search {
        /// Search query (value, package, category, MPN, etc.)
        query: String,

        /// Output format (human, json)
        #[arg(short, long, default_value = "human")]
        format: String,

        /// Only show JLCPCB basic parts (lower assembly fee)
        #[arg(short, long)]
        basic: bool,

        /// Include preferred/promotional parts (requires --basic)
        #[arg(short, long, requires = "basic")]
        preferred: bool,

        /// Maximum number of results per page
        #[arg(short, long, default_value = "50")]
        limit: usize,

        /// Page number (1-indexed)
        #[arg(long, default_value = "1")]
        page: i32,
    },

    /// Generate .zen component files from JLCPCB parts
    Generate {
        /// LCSC part number(s) (e.g., C307331)
        #[arg(required = true)]
        lcsc: Vec<String>,

        /// Output directory (default: components/JLCPCB/<mpn>/)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Component name override (only for single part)
        #[arg(short, long)]
        name: Option<String>,

        /// Ignore cache, re-fetch pins from EasyEDA
        #[arg(long)]
        refresh: bool,
    },

    /// BOM operations for JLCPCB assembly
    Bom {
        #[command(subcommand)]
        command: BomCommands,
    },
}

#[derive(Subcommand)]
enum BomCommands {
    /// Check BOM availability against JLCPCB inventory
    Check {
        /// Path to BOM file (.json or .zen)
        bom: PathBuf,

        /// Quantity of boards to build
        #[arg(short, long, default_value = "100")]
        quantity: i32,
    },

    /// Export BOM in JLCPCB assembly format
    Export {
        /// Path to BOM file (.json or .zen)
        bom: PathBuf,

        /// Output CSV file path
        #[arg(short, long, default_value = "jlcpcb_bom.csv")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Search {
            query,
            format,
            basic,
            preferred,
            limit,
            page,
        } => {
            let output_format = match format.to_lowercase().as_str() {
                "json" => commands::search::OutputFormat::Json,
                _ => commands::search::OutputFormat::Human,
            };

            let library_type = if basic && preferred {
                api::LibraryType::BasicAndPreferred
            } else if basic {
                api::LibraryType::Basic
            } else {
                api::LibraryType::All
            };

            commands::search::execute(&query, output_format, library_type, limit, page)
        }

        Commands::Generate {
            lcsc,
            output,
            name,
            refresh,
        } => {
            let options = pins::ExtractionOptions { refresh };

            if lcsc.len() == 1 {
                commands::generate::execute(&lcsc[0], output, name, &options)
            } else {
                if name.is_some() {
                    eprintln!("Warning: --name is ignored when generating multiple parts");
                }
                commands::generate::execute_batch(&lcsc, output, &options)
            }
        }

        Commands::Bom { command } => match command {
            BomCommands::Check { bom, quantity } => {
                commands::bom::execute_check(&bom, quantity)
            }
            BomCommands::Export { bom, output } => {
                commands::bom::execute_export(&bom, &output)
            }
        },
    }
}
