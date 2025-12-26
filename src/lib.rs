//! A small Rust client for the Copernicus Climate Data Store (CDS) API.
//!
//! This crate implements a `cdsapi`-style flow:
//! submit a request, poll for completion, then download the resulting file.
//!
//! ## Quick start
//! - Configure authentication via environment variables (`CDSAPI_URL`, `CDSAPI_KEY`) or a
//!   `.cdsapirc` file (supported in the current directory and in your home directory).
//! - Call [`Client::retrieve`] with a dataset and a JSON request.
//!
//! ```no_run
//! use anyhow::Result;
//! use cdsapi::Client;
//! use serde_json::json;
//!
//! fn main() -> Result<()> {
//!     let client = Client::from_env()?;
//!     let request = json!({
//!         "product_type": ["reanalysis"],
//!         "variable": ["geopotential"],
//!         "year": ["2024"],
//!         "month": ["03"],
//!         "day": ["01"],
//!         "time": ["13:00"],
//!         "pressure_level": ["1000"],
//!         "data_format": "grib"
//!     });
//!     client.retrieve(
//!         "reanalysis-era5-pressure-levels",
//!         &request,
//!         Some(std::path::Path::new("download.grib")),
//!     )?;
//!     Ok(())
//! }
//! ```
//!
//! For full usage and configuration details, see the crate README.

#![forbid(unsafe_code)]

mod client;
mod config;
mod download;
mod error;
mod legacy;
mod processing;
mod util;

pub use client::{Client, ClientConfig, RemoteFile};
