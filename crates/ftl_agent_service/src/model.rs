use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ftl_agent_core::Model;
use serde_json::{Value, json};
use std::time::Duration;
use url::{Host, Url};

pub struct LocalModel {
    client: reqwest::Client,
    endpoint: Url,
    model: String,
}

pub fn validate_endpoint(endpoint: &str) -> Result<Url> {
    let url = Url::parse(endpoint)?;
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !loopback
        || url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().trim_end_matches('/') != "/v1"
    {
        bail!(
            "endpoint must be literal loopback HTTP with path /v1 and no credentials, query, or fragment"
        )
    }
    Ok(url)
}

impl LocalModel {
    pub fn new(endpoint: &str, model: String) -> Result<Self> {
        if model.is_empty() || model.len() > 256 {
            bail!("explicit model id is required")
        }
        let mut endpoint = validate_endpoint(endpoint)?;
        endpoint.set_path("/v1/chat/completions");
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(180))
            .build()?;
        Ok(Self {
            client,
            endpoint,
            model,
        })
    }
}

#[async_trait]
impl Model for LocalModel {
    async fn complete(&self, messages: &[Value], tools: &[Value]) -> Result<Value> {
        let mut body =
            json!({"model":self.model,"messages":messages,"stream":false,"max_tokens":4096});
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("local model request failed with HTTP {}", response.status())
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                bail!("model response exceeds byte limit")
            }
            bytes.extend_from_slice(&chunk);
        }
        let response: Value = serde_json::from_slice(&bytes).context("invalid model JSON")?;
        let message = response["choices"][0]["message"].clone();
        if message["role"] != "assistant" {
            bail!("model did not return an assistant message")
        }
        Ok(message)
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
