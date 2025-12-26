use anyhow::{Context, Result, anyhow, bail};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::StatusCode;
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::header::{HeaderMap, HeaderValue, RANGE, USER_AGENT};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::config::load_config;
use crate::error::{CdsErrorResponse, format_cds_error};
use crate::legacy::{ApiReply, remote_file_from_reply};
use crate::processing::{ProcessingJob, ProcessingJobStatus, ProcessingResults};
use crate::util::{
    api_v2_variant, append_query, backoff, extract_http_status, guess_filename_from_url,
    retriable_status, split_key_basic,
};

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base CDS API URL, typically `https://cds.climate.copernicus.eu/api`.
    pub url: String,
    /// API key.
    ///
    /// Supported formats:
    /// - Legacy: `<UID>:<APIKEY>`
    /// - Token-only: `<PERSONAL_ACCESS_TOKEN>` (no colon)
    pub key: String,
    /// Whether to verify TLS certificates.
    pub verify: bool,
}

#[derive(Debug, Clone)]
pub struct Client {
    url: String,
    key: String,

    timeout: Duration,
    retry_max: usize,
    sleep_max: Duration,
    wait_until_complete: bool,
    progress: bool,

    http: HttpClient,
}

#[derive(Debug, Clone)]
pub struct RemoteFile {
    /// Download URL.
    pub location: String,
    /// Expected content length (bytes).
    pub content_length: u64,
    /// Optional content type.
    pub content_type: Option<String>,
}

impl Client {
    /// Creates a client using environment variables and/or `.cdsapirc`.
    ///
    /// This is equivalent to `Client::new(None, None, None)`.
    pub fn from_env() -> Result<Self> {
        Self::new(None, None, None)
    }

    /// Creates a client using (in order of precedence):
    /// - explicit `url`/`key` arguments
    /// - environment variables `CDSAPI_URL` / `CDSAPI_KEY`
    /// - config file from `CDSAPI_RC` or `.cdsapirc`
    pub fn new(url: Option<String>, key: Option<String>, verify: Option<bool>) -> Result<Self> {
        let cfg = load_config(url, key, verify)?;

        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("cdsapi-rs/{}", env!("CARGO_PKG_VERSION")))
                .unwrap_or(HeaderValue::from_static("cdsapi-rs")),
        );

        let mut builder = HttpClient::builder()
            .default_headers(default_headers)
            .timeout(Duration::from_secs(60));

        if !cfg.verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder.build().context("failed to build HTTP client")?;

        Ok(Self {
            url: cfg.url,
            key: cfg.key,
            timeout: Duration::from_secs(60),
            retry_max: 500,
            sleep_max: Duration::from_secs(120),
            wait_until_complete: true,
            progress: true,
            http,
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_retry_max(mut self, retry_max: usize) -> Self {
        self.retry_max = retry_max;
        self
    }

    pub fn with_sleep_max(mut self, sleep_max: Duration) -> Self {
        self.sleep_max = sleep_max;
        self
    }

    pub fn with_wait_until_complete(mut self, wait: bool) -> Self {
        self.wait_until_complete = wait;
        self
    }

    pub fn with_progress(mut self, progress: bool) -> Self {
        self.progress = progress;
        self
    }

    /// Submits a request and downloads the resulting file.
    ///
    /// Equivalent to Python: `client.retrieve(dataset, request, target)`.
    pub fn retrieve<T: Serialize>(
        &self,
        dataset: &str,
        request: &T,
        target: Option<&Path>,
    ) -> Result<RemoteFile> {
        // CDS API has two auth/key formats in the wild:
        // - Legacy: "<UID>:<APIKEY>" -> uses /resources + /tasks
        // - Modern: "<PERSONAL-ACCESS-TOKEN>" (no colon) -> uses Retrieve API (/api/retrieve/v1)
        if split_key_basic(&self.key).is_some() {
            return self.retrieve_legacy(dataset, request, target);
        }

        self.retrieve_processing(dataset, request, target)
    }

    fn retrieve_legacy<T: Serialize>(
        &self,
        dataset: &str,
        request: &T,
        target: Option<&Path>,
    ) -> Result<RemoteFile> {
        // CDS has historically been available under both `/api` and `/api/v2`.
        // Some environments now require `/api/v2`, so we auto-fallback on 404.
        let (base_url, mut reply) = self.post_with_base_fallback(dataset, request)?;

        if !self.wait_until_complete {
            let file = remote_file_from_reply(&reply, &base_url)?;
            if let Some(target) = target {
                self.download(&file, target)?;
            }
            return Ok(file);
        }

        let mut sleep = Duration::from_secs(1);
        let mut last_state: Option<String> = None;

        loop {
            if last_state.as_deref() != Some(reply.state.as_str()) {
                last_state = Some(reply.state.clone());
                eprintln!("Request state: {}", reply.state);
            }

            match reply.state.as_str() {
                "completed" => {
                    let file = remote_file_from_reply(&reply, &base_url)?;
                    if let Some(target) = target {
                        self.download(&file, target)?;
                    }
                    return Ok(file);
                }
                "queued" | "running" => {
                    let rid = reply
                        .request_id
                        .clone()
                        .ok_or_else(|| anyhow!("missing request_id while state={}", reply.state))?;
                    thread::sleep(sleep);
                    sleep = backoff(sleep, self.sleep_max);

                    let task_url = format!("{}/tasks/{}", base_url.trim_end_matches('/'), rid);
                    reply = self.api_json::<Value, ApiReply>("GET", &task_url, &Value::Null)?;
                }
                "failed" => {
                    let msg = reply
                        .error
                        .as_ref()
                        .and_then(|e| e.message.as_deref())
                        .unwrap_or("request failed");
                    let reason = reply
                        .error
                        .as_ref()
                        .and_then(|e| e.reason.as_deref())
                        .unwrap_or("");
                    bail!(
                        "{}{}{}",
                        msg,
                        if reason.is_empty() { "" } else { ". " },
                        reason
                    );
                }
                other => bail!("unknown API state [{}]", other),
            }
        }
    }

    fn retrieve_processing<T: Serialize>(
        &self,
        dataset: &str,
        request: &T,
        target: Option<&Path>,
    ) -> Result<RemoteFile> {
        // Modern Retrieve API (OGC API - Processes):
        // POST /api/retrieve/v1/processes/{process_id}/execution {"inputs": <request>}
        // then poll until status==successful, then GET results.
        let base = self.url.trim_end_matches('/');
        let retrieve_base = format!("{}/retrieve/v1", base);
        let exec_url = format!("{}/processes/{}/execution", retrieve_base, dataset);

        let submit_body = serde_json::json!({ "inputs": request });
        let job: ProcessingJob = self.api_json("POST", &exec_url, &submit_body)?;

        let monitor_url = job
            .monitor_url()
            .or_else(|| {
                job.job_id
                    .as_deref()
                    .map(|id| format!("{}/jobs/{}", retrieve_base, id))
            })
            .ok_or_else(|| anyhow!("missing monitor link in job submission response"))?;

        if !self.wait_until_complete {
            bail!(
                "wait_until_complete=false is not yet supported for token-only keys; set wait_until_complete=true"
            );
        }

        let mut sleep = Duration::from_secs(1);
        let mut last_status: Option<String> = None;
        loop {
            let status_url = append_query(&monitor_url, &[("log", "true"), ("request", "true")]);
            let job_status: ProcessingJobStatus =
                self.api_json::<Value, ProcessingJobStatus>("GET", &status_url, &Value::Null)?;

            if last_status.as_deref() != Some(job_status.status.as_str()) {
                last_status = Some(job_status.status.clone());
                eprintln!("Job status: {}", job_status.status);
            }

            match job_status.status.as_str() {
                "successful" => {
                    let results_url = job_status.results_url().unwrap_or_else(|| {
                        format!("{}/results", monitor_url.trim_end_matches('/'))
                    });
                    let results: ProcessingResults = self.api_json::<Value, ProcessingResults>(
                        "GET",
                        &results_url,
                        &Value::Null,
                    )?;
                    let file = results.to_remote_file(&results_url)?;
                    if let Some(target) = target {
                        self.download(&file, target)?;
                    }
                    return Ok(file);
                }
                "accepted" | "running" => {
                    thread::sleep(sleep);
                    sleep = backoff(sleep, self.sleep_max);
                }
                "failed" | "rejected" | "dismissed" | "deleted" => {
                    bail!("processing failed with status {}", job_status.status);
                }
                other => bail!("unknown processing status [{}]", other),
            }
        }
    }

    fn post_with_base_fallback<T: Serialize>(
        &self,
        dataset: &str,
        request: &T,
    ) -> Result<(String, ApiReply)> {
        let base = self.url.trim_end_matches('/').to_string();
        let url = format!("{}/resources/{}", base, dataset);

        match self.api_json::<T, ApiReply>("POST", &url, request) {
            Ok(reply) => Ok((base, reply)),
            Err(e) => {
                // If we got a 404 from the server, try the `/v2` variant.
                if let Some(StatusCode::NOT_FOUND) = extract_http_status(&e) {
                    if !base.contains("/api/v2") {
                        if let Some(alt_base) = api_v2_variant(&base) {
                            let alt_url = format!("{}/resources/{}", alt_base, dataset);
                            if let Ok(reply) =
                                self.api_json::<T, ApiReply>("POST", &alt_url, request)
                            {
                                return Ok((alt_base, reply));
                            }
                        }
                    }
                }
                Err(e)
            }
        }
    }

    pub fn download(&self, file: &RemoteFile, target: &Path) -> Result<PathBuf> {
        let target = if target.as_os_str().is_empty() {
            guess_filename_from_url(&file.location)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("download"))
        } else {
            target.to_path_buf()
        };

        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory {}", parent.display()))?;
            }
        }

        let mut downloaded: u64 = 0;
        let mut mode_append = false;
        let mut range_from: Option<u64> = None;

        if target.exists() {
            downloaded = std::fs::metadata(&target)?.len();
            if downloaded < file.content_length {
                mode_append = true;
                range_from = Some(downloaded);
            }
        }

        let pb = if self.progress {
            let pb = ProgressBar::new(file.content_length);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {bytes}/{total_bytes} ({bytes_per_sec}) {wide_bar} {eta}",
                )
                .unwrap()
                .progress_chars("=>-"),
            );
            pb.set_position(downloaded);
            Some(pb)
        } else {
            None
        };

        let mut tries = 0usize;
        'download_attempt: while tries < self.retry_max {
            let mut headers = HeaderMap::new();
            if let Some(from) = range_from {
                headers.insert(RANGE, HeaderValue::from_str(&format!("bytes={}-", from))?);
            }

            let resp = self.robust_request(|| {
                let mut req = self.http.get(&file.location).headers(headers.clone());
                req = self.apply_auth(req);
                req.send()
            })?;

            let mut resp = resp.error_for_status().context("download request failed")?;
            let mut out = OpenOptions::new()
                .create(true)
                .write(true)
                .append(mode_append)
                .truncate(!mode_append)
                .open(&target)
                .with_context(|| format!("failed to open {}", target.display()))?;

            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = match resp.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => {
                        tries += 1;
                        if tries >= self.retry_max {
                            return Err(e).context("download interrupted")?;
                        }

                        // resume
                        out.flush().ok();
                        downloaded = std::fs::metadata(&target)?.len();
                        range_from = Some(downloaded);
                        mode_append = true;
                        if let Some(pb) = &pb {
                            pb.set_position(downloaded);
                        }
                        thread::sleep(self.sleep_max);
                        continue 'download_attempt;
                    }
                };

                out.write_all(&buf[..n])?;
                downloaded += n as u64;
                if let Some(pb) = &pb {
                    pb.inc(n as u64);
                }
            }

            out.flush()?;

            if downloaded >= file.content_length {
                if let Some(pb) = &pb {
                    pb.finish_and_clear();
                }
                return Ok(target);
            }

            tries += 1;
            // resume and retry
            downloaded = std::fs::metadata(&target)?.len();
            range_from = Some(downloaded);
            mode_append = true;
            if let Some(pb) = &pb {
                pb.set_position(downloaded);
            }
            thread::sleep(self.sleep_max);
        }

        bail!(
            "download failed: downloaded {} byte(s) out of {}",
            downloaded,
            file.content_length
        )
    }

    fn apply_auth(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some((u, p)) = split_key_basic(&self.key) {
            req.basic_auth(u, Some(p))
        } else {
            // Modern APIs use a custom header.
            req.header("PRIVATE-TOKEN", self.key.trim())
        }
    }

    fn api_json<TReq: Serialize, TResp: DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        request: &TReq,
    ) -> Result<TResp> {
        let resp = self.robust_request(|| {
            let req = match method {
                "GET" => self.http.get(url),
                "PUT" => self.http.put(url),
                _ => self.http.post(url),
            };
            let req = self.apply_auth(req);
            if method == "GET" {
                req.send()
            } else {
                req.json(request).send()
            }
        })?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            // Try to parse CDS error payloads for actionable messages.
            if let Ok(err_json) = serde_json::from_str::<CdsErrorResponse>(&text) {
                return Err(format_cds_error(status, url, &err_json).into());
            }

            bail!(
                "API request failed: HTTP {} for url ({})\n{}",
                status,
                url,
                text
            );
        }

        serde_json::from_str::<TResp>(&text)
            .with_context(|| format!("failed to parse API JSON (url={}, status={})", url, status))
    }

    fn robust_request<F>(&self, mut f: F) -> Result<Response>
    where
        F: FnMut() -> std::result::Result<Response, reqwest::Error>,
    {
        let mut tries = 0usize;
        loop {
            let result = f();

            match result {
                Ok(resp) => {
                    if retriable_status(resp.status().as_u16()) {
                        tries += 1;
                        if tries >= self.retry_max {
                            return Ok(resp);
                        }
                        thread::sleep(self.sleep_max);
                        continue;
                    }
                    return Ok(resp);
                }
                Err(err) => {
                    tries += 1;
                    if tries >= self.retry_max {
                        return Err(err).context("could not connect")?;
                    }
                    // timeouts / transient connection errors
                    thread::sleep(self.sleep_max);
                }
            }
        }
    }
}
