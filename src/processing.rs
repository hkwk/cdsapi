use anyhow::{Result, anyhow};

use crate::client::RemoteFile;
use crate::util::urljoin;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProcessingLink {
    #[serde(default)]
    rel: Option<String>,
    href: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProcessingJob {
    #[serde(default, alias = "jobID")]
    pub(crate) job_id: Option<String>,
    #[serde(default)]
    links: Vec<ProcessingLink>,
}

impl ProcessingJob {
    pub(crate) fn monitor_url(&self) -> Option<String> {
        self.links
            .iter()
            .find(|l| l.rel.as_deref() == Some("monitor"))
            .map(|l| l.href.clone())
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProcessingJobStatus {
    pub(crate) status: String,
    #[serde(default)]
    links: Vec<ProcessingLink>,
}

impl ProcessingJobStatus {
    pub(crate) fn results_url(&self) -> Option<String> {
        self.links
            .iter()
            .find(|l| l.rel.as_deref() == Some("results"))
            .map(|l| l.href.clone())
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProcessingResults {
    asset: ProcessingAsset,
}

#[derive(Debug, serde::Deserialize)]
struct ProcessingAsset {
    value: ProcessingAssetValue,
}

#[derive(Debug, serde::Deserialize)]
struct ProcessingAssetValue {
    href: String,
    #[serde(rename = "file:size")]
    file_size: u64,
    #[serde(rename = "type")]
    content_type: String,
}

impl ProcessingResults {
    pub(crate) fn to_remote_file(&self, results_url: &str) -> Result<RemoteFile> {
        let href = self.asset.value.href.trim();
        if href.is_empty() {
            return Err(anyhow!("missing results asset href"));
        }

        Ok(RemoteFile {
            location: urljoin(results_url, href),
            content_length: self.asset.value.file_size,
            content_type: Some(self.asset.value.content_type.clone()),
        })
    }
}
