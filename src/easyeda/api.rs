//! EasyEDA API client.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;

/// EasyEDA API endpoint for component data.
const EASYEDA_API_URL: &str = "https://easyeda.com/api/products";

/// API version parameter.
const API_VERSION: &str = "6.4.19.5";

/// EasyEDA API client.
pub struct EasyEdaClient {
    client: Client,
}

impl EasyEdaClient {
    /// Create a new EasyEDA client.
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    /// Fetch component data from EasyEDA.
    ///
    /// Returns the raw component data including symbol shapes.
    pub fn get_component(&self, lcsc_id: &str) -> Result<Option<ComponentData>> {
        let url = format!(
            "{}/{}/components?version={}",
            EASYEDA_API_URL, lcsc_id, API_VERSION
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "pcb-jlcpcb")
            .send()
            .context("Failed to fetch component from EasyEDA")?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let api_response: ApiResponse = response
            .json()
            .context("Failed to parse EasyEDA response")?;

        if !api_response.success {
            return Ok(None);
        }

        Ok(api_response.result)
    }
}

/// EasyEDA API response wrapper.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    success: bool,
    result: Option<ComponentData>,
}

/// Component data from EasyEDA.
#[derive(Debug, Deserialize)]
pub struct ComponentData {
    /// Component UUID.
    pub uuid: String,

    /// Component title (usually the MPN).
    pub title: String,

    /// Raw data string containing symbol information.
    #[serde(rename = "dataStr")]
    pub data_str: Option<DataStr>,

    /// Package/footprint details.
    #[serde(rename = "packageDetail")]
    pub package_detail: Option<PackageDetail>,
}

/// Symbol data structure.
#[derive(Debug, Deserialize)]
pub struct DataStr {
    /// Shape elements including pins.
    pub shape: Option<Vec<String>>,
}

/// Package/footprint details.
#[derive(Debug, Deserialize)]
pub struct PackageDetail {
    /// Footprint UUID.
    pub uuid: String,

    /// Footprint name (e.g., "WLP-9_L1.4-W1.3-P0.40-R3-C3-BR").
    pub title: String,

    /// Footprint data.
    #[serde(rename = "dataStr")]
    pub data_str: Option<PackageDataStr>,
}

/// Package data structure.
#[derive(Debug, Deserialize)]
pub struct PackageDataStr {
    /// Package head with metadata.
    pub head: Option<PackageHead>,

    /// Shape elements for the footprint.
    pub shape: Option<Vec<String>>,
}

/// Package head metadata.
#[derive(Debug, Deserialize)]
pub struct PackageHead {
    /// Package parameters.
    pub c_para: Option<PackageParams>,

    /// 3D model UUID.
    pub uuid_3d: Option<String>,
}

/// Package parameters.
#[derive(Debug, Deserialize)]
pub struct PackageParams {
    /// Package name.
    pub package: Option<String>,

    /// 3D model name.
    #[serde(rename = "3DModel")]
    pub model_3d: Option<String>,
}
