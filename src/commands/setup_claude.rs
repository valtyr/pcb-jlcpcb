use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::CommandFactory;
use colored::Colorize;

use crate::Cli;

pub fn execute() -> Result<()> {
    let path = PathBuf::from(".claude/skills/pcb-jlcpcb/SKILL.md");
    fs::create_dir_all(path.parent().unwrap())?;
    let content = generate_skill();
    fs::write(&path, &content)?;
    println!(
        "{} Wrote {}",
        "✓".green(),
        path.display().to_string().cyan()
    );
    Ok(())
}

fn generate_skill() -> String {
    let mut out = String::new();

    // YAML frontmatter
    out.push_str(
        "---\n\
         name: pcb-jlcpcb\n\
         description: Search JLCPCB parts, generate components, and check/export BOMs\n\
         allowed-tools: Bash(pcb jlcpcb *)\n\
         ---\n\n",
    );

    // Intro
    out.push_str(
        "# pcb jlcpcb\n\n\
         CLI tool for working with the JLCPCB parts library. \
         Search for components, generate `.zen` files, and manage BOMs for JLCPCB assembly.\n\n\
         Invoke as `pcb jlcpcb <subcommand>`.\n\n",
    );

    // Dynamic command reference
    let cmd = Cli::command();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" || name == "setup-claude" {
            continue;
        }

        // Top-level heading
        writeln!(out, "## {name}\n").unwrap();

        // Render help in a code block
        let help = sub.clone().render_long_help().to_string();
        writeln!(out, "```\n{help}```\n").unwrap();

        // Recurse one level for nested subcommands (e.g. bom check, bom export)
        for child in sub.get_subcommands() {
            let child_name = child.get_name();
            if child_name == "help" {
                continue;
            }

            writeln!(out, "### {name} {child_name}\n").unwrap();

            let child_help = child.clone().render_long_help().to_string();
            writeln!(out, "```\n{child_help}```\n").unwrap();
        }
    }

    // Tips section
    out.push_str(
        "## Tips\n\n\
         - **Always use `--format json`** (or `-f json`) when running `bom check`, `bom export`, \
           or `search` commands so you can parse the structured output.\n\
         - Use `--basic` when searching to find parts with the lowest assembly fee.\n\
         - You can generate multiple components at once: `pcb jlcpcb generate C307331 C123456`.\n\
         - Always run `pcb jlcpcb bom check` before exporting to verify stock availability.\n\
         - Use `--refresh` on bom commands to bypass the 24-hour part cache.\n",
    );

    out
}
