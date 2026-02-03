//! Pin extraction module for JLCPCB components.
//!
//! This module handles extracting pin information from component datasheets using:
//! 1. A local cache to avoid repeated extraction
//! 2. Ollama vision model for PDF analysis

mod cache;
mod extract;

pub use extract::{extract_pins, ExtractionOptions};
