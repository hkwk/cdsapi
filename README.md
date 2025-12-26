# cdsapi (Rust)

A Rust client for the Copernicus Climate Data Store (CDS) API, inspired by the Python `cdsapi` library.

This crate supports both CDS key formats currently seen in the wild:

- **Legacy key**: `<UID>:<APIKEY>` → uses the legacy `/resources` + `/tasks` workflow.
- **Token-only key**: `<PERSONAL_ACCESS_TOKEN>` (no colon) → uses the modern Retrieve API (`/api/retrieve/v1`) with `PRIVATE-TOKEN` auth.

## Configuration

The client loads configuration from (highest precedence first):

1. Environment variables:
   - `CDSAPI_URL`
   - `CDSAPI_KEY`
   - `CDSAPI_RC` (path to a config file)
2. Config file in the current working directory: `./.cdsapirc`
3. Config file in the home directory: `~/.cdsapirc`

Example `.cdsapirc`:

```yaml
url: https://cds.climate.copernicus.eu/api
key: <PERSONAL_ACCESS_TOKEN>
# or legacy:
# key: <UID>:<APIKEY>
```

Notes:
- The parser is lenient and also accepts `key:` on one line and the value on the next line.
- Set `verify: 0` to disable TLS certificate validation (not recommended).

## Usage

Library usage:

```rust
use anyhow::Result;
use cdsapi::Client;
use serde_json::json;

fn main() -> Result<()> {
  let client = Client::from_env()?;

    let request = json!({
        "product_type": "reanalysis",
        "variable": "geopotential",
        "pressure_level": "1000",
        "year": "2024",
        "month": "03",
        "day": "01",
        "time": "13:00",
        "format": "grib"
    });

    client.retrieve(
        "reanalysis-era5-pressure-levels",
        &request,
        Some(std::path::Path::new("download.grib")),
    )?;

    Ok(())
}
```

Example program:

```bash
cargo run --example era5_pressure_levels_geopotential
```

## Runtime output

The client prints request/job status transitions to stderr while polling (for example: `Request state: running` or `Job status: accepted`).

## Troubleshooting

- **403 required licences not accepted**:
  - Sign in to the CDS web UI, open the dataset page, and accept the required licence(s) (often via “Manage licences”), then retry.
- **401/403 auth failures**:
  - Ensure your key is correct. Many token-only keys should NOT include the deprecated `<UID>:` prefix.
- **404 endpoint not found**:
  - Ensure `url` is `https://cds.climate.copernicus.eu/api` (or your CDS deployment base URL).

## License

Licensed under the Apache License, Version 2.0. See `LICENSE` and `NOTICE`.
