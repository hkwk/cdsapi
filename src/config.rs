use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use crate::client::ClientConfig;

#[derive(Debug, Default)]
struct RcConfig {
    url: Option<String>,
    key: Option<String>,
    verify: Option<bool>,
}

pub(crate) fn load_config(
    url: Option<String>,
    key: Option<String>,
    verify: Option<bool>,
) -> Result<ClientConfig> {
    let mut url = url.or_else(|| std::env::var("CDSAPI_URL").ok());
    let mut key = key.or_else(|| std::env::var("CDSAPI_KEY").ok());

    let rc_candidates = rc_candidates();
    let mut file_verify: Option<bool> = None;

    if url.is_none() || key.is_none() || verify.is_none() {
        for rc_path in &rc_candidates {
            if rc_path.exists() {
                let cfg = read_rc(rc_path).with_context(|| {
                    format!("failed to read configuration file {}", rc_path.display())
                })?;

                if url.is_none() {
                    url = cfg.url;
                }
                if key.is_none() {
                    key = cfg.key;
                }
                file_verify = cfg.verify;
                break;
            }
        }
    }

    let url = match url {
        Some(v) => v,
        None => {
            if !rc_candidates.is_empty() {
                bail!(
                    "Missing configuration: url (set CDSAPI_URL or put `url:` in one of: {})",
                    rc_candidates
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            bail!("Missing configuration: url (set CDSAPI_URL or create .cdsapirc)");
        }
    };

    let key = match key {
        Some(v) => v,
        None => {
            if !rc_candidates.is_empty() {
                bail!(
                    "Missing configuration: key (set CDSAPI_KEY or put `key:` in one of: {})",
                    rc_candidates
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            bail!("Missing configuration: key (set CDSAPI_KEY or create .cdsapirc)");
        }
    };

    let verify = verify.or(file_verify).unwrap_or(true);

    Ok(ClientConfig { url, key, verify })
}

fn read_rc(path: &Path) -> Result<RcConfig> {
    let text = std::fs::read_to_string(path)?;
    let mut cfg = RcConfig::default();

    // Support formatting where `key:` is on one line and the token is on the next line.
    let mut pending_key: Option<&str> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(pk) = pending_key {
            // Continuation value line (no colon)
            if !line.contains(':') {
                let v = strip_quotes(line);
                match pk {
                    "url" => cfg.url = Some(v.to_string()),
                    "key" => cfg.key = Some(v.to_string()),
                    _ => {}
                }
                pending_key = None;
                continue;
            }
            pending_key = None;
        }

        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = strip_quotes(v.trim());
            match k {
                "url" => {
                    if !v.is_empty() {
                        cfg.url = Some(v.to_string());
                    } else {
                        pending_key = Some("url");
                    }
                }
                "key" => {
                    if !v.is_empty() {
                        cfg.key = Some(v.to_string());
                    } else {
                        pending_key = Some("key");
                    }
                }
                "verify" => {
                    if !v.is_empty() {
                        cfg.verify = Some(v != "0");
                    }
                }
                _ => {}
            }
        }
    }

    Ok(cfg)
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn rc_candidates() -> Vec<PathBuf> {
    // Search order compatible with Python cdsapi plus an extra convenience:
    // 1) CDSAPI_RC (explicit)
    // 2) ./.cdsapirc (execution directory / current working directory)
    // 3) ~/.cdsapirc
    if let Ok(p) = std::env::var("CDSAPI_RC") {
        return vec![PathBuf::from(p)];
    }

    let mut v = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        v.push(cwd.join(".cdsapirc"));
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".cdsapirc"));
    }
    v
}
