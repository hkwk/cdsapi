use anyhow::{Result, bail};
use serde_json::Value;

use crate::client::RemoteFile;
use crate::util::urljoin;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ApiReply {
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) request_id: Option<String>,

    #[serde(default)]
    pub(crate) location: Option<String>,
    #[serde(default, alias = "contentLength", alias = "content_length")]
    pub(crate) content_length: Option<u64>,
    #[serde(default, alias = "contentType", alias = "content_type")]
    pub(crate) content_type: Option<String>,

    #[serde(default)]
    pub(crate) result: Option<Value>,

    #[serde(default)]
    pub(crate) error: Option<ApiError>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ApiError {
    #[serde(default)]
    pub(crate) message: Option<String>,
    #[serde(default)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiResultLocation {
    location: String,
    #[serde(alias = "contentLength", alias = "content_length")]
    content_length: u64,
    #[serde(default, alias = "contentType", alias = "content_type")]
    content_type: Option<String>,
}

pub(crate) fn remote_file_from_reply(reply: &ApiReply, base_url: &str) -> Result<RemoteFile> {
    // 1) If API returns {"result": {"location":...,"contentLength":...}}
    if let Some(result) = &reply.result {
        if let Ok(r) = serde_json::from_value::<ApiResultLocation>(result.clone()) {
            return Ok(RemoteFile {
                location: urljoin(base_url, &r.location),
                content_length: r.content_length,
                content_type: r.content_type,
            });
        }
    }

    // 2) Or it returns location/contentLength at top-level
    if let (Some(location), Some(content_length)) = (&reply.location, reply.content_length) {
        return Ok(RemoteFile {
            location: urljoin(base_url, location),
            content_length,
            content_type: reply.content_type.clone(),
        });
    }

    bail!("missing download info in API reply")
}
