//! Client de l'API publique (contrat compatible OpenAI), utilisé par
//! `chat`, `test` et `doctor`.

use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::Config;
use crate::error::Fail;
use crate::gateway::{Health, http_client};
use crate::paths::Paths;
use crate::secrets::Secrets;
use crate::sse::SseParser;

pub struct Api {
    http: reqwest::Client,
    pub base_url: String,
    key: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StreamStats {
    pub ttft_ms: Option<u128>,
    pub total_ms: u128,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: u64,
    /// Vrai si le nombre de tokens vient de l'API (sinon : nombre de fragments).
    pub usage_reported: bool,
    pub tokens_per_second: f64,
    pub done_marker: bool,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Delta<'a> {
    Content(&'a str),
    Reasoning(&'a str),
}

impl Api {
    /// Clé : variable ATELIER_API_KEY si définie, sinon clé maître (root).
    pub fn local(cfg: &Config, paths: &Paths) -> anyhow::Result<Self> {
        let key = match std::env::var("ATELIER_API_KEY") {
            Ok(k) if !k.is_empty() => k,
            _ => {
                paths.require_privileges().map_err(|_| {
                    Fail::Permission(format!(
                        "lecture de la clé maître impossible : relancez avec sudo ou définissez ATELIER_API_KEY=<clé> ({})",
                        crate::APP
                    ))
                })?;
                Secrets::load(paths)?.master_key().to_string()
            }
        };
        Ok(Api {
            http: http_client(Duration::from_secs(cfg.limits.timeout as u64)),
            base_url: cfg.local_url(),
            key,
        })
    }

    pub async fn health(&self) -> anyhow::Result<(u16, Option<Health>)> {
        let r = self
            .http
            .get(format!("{}/health", self.base_url))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| Fail::Unhealthy(format!("API injoignable sur {} ({e})", self.base_url)))?;
        let status = r.status().as_u16();
        Ok((status, r.json::<Health>().await.ok()))
    }

    /// Code HTTP de `GET /v1/models` sans clé (doit être 401).
    pub async fn models_without_key(&self) -> anyhow::Result<u16> {
        Ok(self
            .http
            .get(format!("{}/v1/models", self.base_url))
            .send()
            .await?
            .status()
            .as_u16())
    }

    pub async fn models(&self) -> anyhow::Result<Vec<String>> {
        let v: Value = self
            .call(self.http.get(format!("{}/v1/models", self.base_url)))
            .await?;
        Ok(v["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }
    pub async fn embeddings(&self, model: &str, input: &str) -> anyhow::Result<usize> {
        let v = self
            .call(
                self.http
                    .post(format!("{}/v1/embeddings", self.base_url))
                    .json(&json!({ "model": model, "input": input })),
            )
            .await?;
        Ok(v.pointer("/data/0/embedding")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0))
    }

    async fn call(&self, req: reqwest::RequestBuilder) -> anyhow::Result<Value> {
        let r = req
            .bearer_auth(&self.key)
            .send()
            .await
            .context("requête vers l'API")?;
        let status = r.status();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            anyhow::bail!("l'API a répondu {status} : {}", error_message(&body));
        }
        Ok(body)
    }

    /// Complétion en streaming ; `on_delta` reçoit chaque fragment.
    pub async fn chat_stream(
        &self,
        model: &str,
        messages: &[Value],
        mut on_delta: impl FnMut(Delta<'_>),
    ) -> anyhow::Result<StreamStats> {
        let start = Instant::now();
        let r = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .bearer_auth(&self.key)
            .json(&json!({
                "model": model,
                "messages": messages,
                "stream": true,
                "stream_options": { "include_usage": true },
            }))
            .send()
            .await
            .context("requête vers l'API")?;
        let status = r.status();
        if !status.is_success() {
            let body: Value = r.json().await.unwrap_or(Value::Null);
            anyhow::bail!("l'API a répondu {status} : {}", error_message(&body));
        }
        let mut stats = StreamStats::default();
        let mut first_token_at = None;
        let mut chunks = 0u64;
        let mut parser = SseParser::default();
        let mut body = r.bytes_stream();
        let mut handle = |data: String, stats: &mut StreamStats| -> anyhow::Result<()> {
            if data.trim() == "[DONE]" {
                stats.done_marker = true;
                return Ok(());
            }
            let v: Value = serde_json::from_str(&data)
                .with_context(|| format!("fragment SSE invalide : {data}"))?;
            if let Some(err) = v.get("error") {
                anyhow::bail!(
                    "erreur dans le flux : {}",
                    error_message(&json!({ "error": err }))
                );
            }
            if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                if let Some(c) = u["completion_tokens"].as_u64() {
                    stats.completion_tokens = c;
                    stats.usage_reported = true;
                }
                stats.prompt_tokens = u["prompt_tokens"].as_u64();
            }
            let delta = &v["choices"][0]["delta"];
            let reasoning = delta["reasoning_content"]
                .as_str()
                .or_else(|| delta["reasoning"].as_str());
            if let Some(t) = reasoning.filter(|t| !t.is_empty()) {
                first_token_at.get_or_insert_with(Instant::now);
                chunks += 1;
                on_delta(Delta::Reasoning(t));
            }
            if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                first_token_at.get_or_insert_with(Instant::now);
                chunks += 1;
                stats.text.push_str(t);
                on_delta(Delta::Content(t));
            }
            Ok(())
        };
        while let Some(chunk) = body.next().await {
            for data in parser.push(&chunk.context("lecture du flux")?) {
                handle(data, &mut stats)?;
            }
        }
        if let Some(data) = parser.finish() {
            handle(data, &mut stats)?;
        }
        let end = Instant::now();
        stats.total_ms = (end - start).as_millis();
        stats.ttft_ms = first_token_at.map(|t| (t - start).as_millis());
        if !stats.usage_reported {
            stats.completion_tokens = chunks;
        }
        if let Some(first) = first_token_at {
            let gen_secs = (end - first).as_secs_f64();
            if gen_secs > 0.0 && stats.completion_tokens > 1 {
                stats.tokens_per_second = (stats.completion_tokens - 1) as f64 / gen_secs;
            }
        }
        Ok(stats)
    }
}

pub fn error_message(body: &Value) -> String {
    body.pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| body.get("detail").map(|d| d.to_string()))
        .unwrap_or_else(|| body.to_string())
}
