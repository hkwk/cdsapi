use reqwest::StatusCode;
use std::time::Duration;

pub(crate) fn retriable_status(code: u16) -> bool {
    matches!(code, 500 | 502 | 503 | 504 | 429 | 408)
}

pub(crate) fn backoff(current: Duration, max: Duration) -> Duration {
    let next = Duration::from_secs_f64((current.as_secs_f64() * 1.5).max(1.0));
    if next > max { max } else { next }
}

pub(crate) fn guess_filename_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    path.rsplit('/').next().and_then(|s| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    })
}

pub(crate) fn split_key_basic(key: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = key.splitn(2, ':').collect();
    if parts.len() == 2 && !parts[0].trim().is_empty() && !parts[1].trim().is_empty() {
        Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
    } else {
        None
    }
}

pub(crate) fn urljoin(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let base = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{}{}", base, path)
    } else {
        format!("{}/{}", base, path)
    }
}

pub(crate) fn append_query(url: &str, params: &[(&str, &str)]) -> String {
    // We only need a tiny helper for "log=true&request=true".
    let mut out = url.to_string();
    let sep = if url.contains('?') { '&' } else { '?' };
    out.push(sep);
    let mut first = true;
    for (k, v) in params {
        if !first {
            out.push('&');
        }
        first = false;
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}

pub(crate) fn api_v2_variant(base: &str) -> Option<String> {
    // Common cases:
    // - https://.../api      -> https://.../api/v2
    // - https://.../api/     -> https://.../api/v2
    let b = base.trim_end_matches('/');
    if b.ends_with("/api") {
        return Some(format!("{}/v2", b));
    }
    // If user set host root, try appending /api/v2
    if !b.contains("/api/") && !b.ends_with("/api/v2") {
        return Some(format!("{}/api/v2", b));
    }
    None
}

pub(crate) fn extract_http_status(err: &anyhow::Error) -> Option<StatusCode> {
    // We format errors including "HTTP <code>" in api_json.
    // Best-effort parse for 404 detection.
    let s = err.to_string();
    if s.contains("HTTP 404") {
        return Some(StatusCode::NOT_FOUND);
    }
    None
}
