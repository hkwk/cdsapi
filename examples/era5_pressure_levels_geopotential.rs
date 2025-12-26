use anyhow::Result;
use cdsapi::Client;
use serde_json::json;
use std::path::Path;

fn main() -> Result<()> {
    // Example program that calls the library API.
    // Configure authentication via env vars or a `.cdsapirc` file.
    let client = Client::from_env()?;

    let dataset = "reanalysis-era5-pressure-levels";
    let request = json!({
        "product_type": ["reanalysis"],
        "variable": ["geopotential"],
        "year": ["2024"],
        "month": ["03"],
        "day": ["01"],
        "time": ["13:00"],
        "pressure_level": ["1000"],
        "data_format": "grib"
    });

    client.retrieve(dataset, &request, Some(Path::new("download.grib")))?;
    Ok(())
}
