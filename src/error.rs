use anyhow::anyhow;
use reqwest::StatusCode;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct CdsErrorResponse {
    #[serde(default, rename = "type")]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<u16>,
    #[serde(default)]
    pub(crate) detail: Option<String>,
    #[serde(default)]
    pub(crate) instance: Option<String>,
    #[serde(default)]
    pub(crate) trace_id: Option<String>,
    // Some endpoints respond with {"message":...,"detail":...}
    #[serde(default)]
    pub(crate) message: Option<String>,
}

pub(crate) fn format_cds_error(
    status: StatusCode,
    url: &str,
    e: &CdsErrorResponse,
) -> anyhow::Error {
    let title = e.title.as_deref().or(e.message.as_deref()).unwrap_or("");
    let detail = e.detail.as_deref().unwrap_or("");
    let trace = e.trace_id.as_deref().unwrap_or("");
    let instance = e.instance.as_deref().unwrap_or("");
    let kind = e.kind.as_deref().unwrap_or("");
    let status_in_body = e.status.unwrap_or(status.as_u16());

    // Licence not accepted is extremely common; provide explicit remediation.
    let looks_like_licence = status == StatusCode::FORBIDDEN
        && (title.to_lowercase().contains("required licences")
            || detail.to_lowercase().contains("required licence")
            || detail.to_lowercase().contains("manage-licences"));
    if looks_like_licence {
        // Try to reuse the link provided by CDS in the detail, otherwise fall back to dataset page.
        let mut link = "https://cds.climate.copernicus.eu/how-to-api".to_string();
        if let Some(idx) = detail.find("https://") {
            link = detail[idx..]
                .split_whitespace()
                .next()
                .unwrap_or(&link)
                .to_string();
        }

        return anyhow!(
            "CDS returned 403: required dataset licence(s) have not been accepted.\n\nHow to fix:\n1) Open and sign in: {}\n2) Scroll to the bottom and accept the required licence(s) (Manage licences)\n3) Re-run this program\n\nServer message: {}\ntrace_id: {}",
            link,
            title,
            if trace.is_empty() { "(none)" } else { trace }
        );
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return anyhow!(
            "CDS authentication/authorization failed (HTTP {}).\n- Check that the key in .cdsapirc is a valid Personal Access Token (often WITHOUT the deprecated '<UID>:' prefix)\n- Ensure the token is not expired\n- If dataset licences are not accepted, CDS returns: 403 required licences not accepted\n\nServer message: {}\n{}\nkind: {}\ninstance: {}\ntrace_id: {}\nrequest: {}",
            status_in_body,
            title,
            detail,
            kind,
            instance,
            if trace.is_empty() { "(none)" } else { trace },
            url
        );
    }

    if status == StatusCode::NOT_FOUND {
        return anyhow!(
            "CDS API endpoint not found (HTTP 404).\n- The API path may have changed, or your configured base URL is incorrect\n- Recommended .cdsapirc url: https://cds.climate.copernicus.eu/api\n\nServer message: {}\n{}\nrequest: {}",
            title,
            detail,
            url
        );
    }

    anyhow!(
        "API request failed: HTTP {} for url ({})\n{}\n{}",
        status_in_body,
        url,
        title,
        detail
    )
}
