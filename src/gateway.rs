//! Client HTTP de la passerelle LiteLLM (administration et API publique).

use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::Config;
use crate::error::{Fail, not_found};
use crate::secrets::Secrets;

/// Nom réservé à la clé interne de l'interface web.
pub const WEBUI_KEY_ALIAS: &str = "open-webui";

pub struct Gateway {
    http: reqwest::Client,
    admin_url: String,
    master_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKey {
    /// Identifiant court (début de l'empreinte).
    pub id: String,
    pub name: String,
    /// Aperçu masqué de la clé (ex. sk-...a1b2).
    pub preview: String,
    pub rpm: Option<u64>,
    pub tpm: Option<u64>,
    pub spend: f64,
    pub created_at: Option<String>,
    pub expires: Option<String>,
    pub last_used: Option<String>,
    #[serde(skip)]
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub rpm: Option<u32>,
    pub tpm: Option<u32>,
    pub expires: Option<String>,
}

pub fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("{}/{}", crate::APP, crate::VERSION))
        .build()
        .expect("client HTTP")
}

impl Gateway {
    pub fn new(cfg: &Config, secrets: &Secrets) -> Self {
        Gateway {
            http: http_client(Duration::from_secs(30)),
            admin_url: cfg.admin_url(),
            master_key: secrets.master_key().to_string(),
        }
    }
    /// Passerelle prête (base connectée).
    pub async fn ready(&self) -> bool {
        matches!(
            self.http.get(format!("{}/health/readiness", self.admin_url)).send().await,
            Ok(r) if r.status().is_success()
        )
    }

    /// Attend que la passerelle soit prête.
    pub async fn wait_ready(&self, timeout: Duration) -> anyhow::Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.ready().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(Fail::Unhealthy(format!(
            "la passerelle n'est pas prête après {} s — voir « {} logs litellm »",
            timeout.as_secs(),
            crate::APP
        ))
        .into())
    }

    async fn admin(&self, req: reqwest::RequestBuilder) -> anyhow::Result<Value> {
        let resp = req
            .bearer_auth(&self.master_key)
            .send()
            .await
            .map_err(|e| {
                Fail::Unhealthy(format!(
                    "passerelle injoignable ({e}) — la pile est-elle démarrée ?"
                ))
            })?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = body
                .pointer("/error/message")
                .or_else(|| body.pointer("/detail"))
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string())
                })
                .unwrap_or_default();
            anyhow::bail!("la passerelle a répondu {status} : {msg}");
        }
        Ok(body)
    }

    pub async fn create_key(
        &self,
        name: &str,
        rpm: Option<u32>,
        tpm: Option<u32>,
        duration: Option<&str>,
        custom_key: Option<&str>,
    ) -> anyhow::Result<CreatedKey> {
        let mut body = json!({ "key_alias": name, "metadata": { "created_by": crate::APP } });
        if let Some(r) = rpm.filter(|r| *r > 0) {
            body["rpm_limit"] = json!(r);
        }
        if let Some(t) = tpm.filter(|t| *t > 0) {
            body["tpm_limit"] = json!(t);
        }
        if let Some(d) = duration {
            body["duration"] = json!(d);
        }
        if let Some(k) = custom_key {
            body["key"] = json!(k);
        }
        let v = self
            .admin(
                self.http
                    .post(format!("{}/key/generate", self.admin_url))
                    .json(&body),
            )
            .await
            .context("création de la clé")?;
        let key = v["key"].as_str().context("réponse sans clé")?.to_string();
        let token = v["token"]
            .as_str()
            .or_else(|| v["token_id"].as_str())
            .unwrap_or_default();
        Ok(CreatedKey {
            id: short_id(token),
            name: name.to_string(),
            key,
            rpm: rpm.filter(|r| *r > 0),
            tpm: tpm.filter(|t| *t > 0),
            expires: v["expires"].as_str().map(str::to_string),
        })
    }

    pub async fn list_keys(&self) -> anyhow::Result<Vec<ApiKey>> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let v = self
                .admin(self.http.get(format!(
                    "{}/key/list?return_full_object=true&size=100&page={page}",
                    self.admin_url
                )))
                .await
                .context("liste des clés")?;
            let keys = v["keys"].as_array().cloned().unwrap_or_default();
            out.extend(keys.iter().filter_map(parse_key));
            let total_pages = v["total_pages"].as_u64().unwrap_or(1);
            if page >= total_pages || keys.is_empty() {
                break;
            }
            page += 1;
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }

    /// Résout un identifiant court, un nom ou une empreinte complète.
    pub async fn resolve_key(&self, id: &str) -> anyhow::Result<ApiKey> {
        let keys = self.list_keys().await?;
        let matches: Vec<&ApiKey> = keys
            .iter()
            .filter(|k| k.token == id || k.name == id || (id.len() >= 6 && k.token.starts_with(id)))
            .collect();
        match matches.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(not_found(format!("aucune clé ne correspond à « {id} »"))),
            _ => Err(crate::error::usage(format!(
                "plusieurs clés correspondent à « {id} », précisez l'identifiant"
            ))),
        }
    }

    pub async fn delete_key(&self, token: &str) -> anyhow::Result<()> {
        self.admin(
            self.http
                .post(format!("{}/key/delete", self.admin_url))
                .json(&json!({ "keys": [token] })),
        )
        .await
        .context("révocation de la clé")?;
        Ok(())
    }

    /// Crée la clé de l'interface web si elle n'existe pas encore.
    pub async fn ensure_webui_key(&self, secrets: &Secrets) -> anyhow::Result<()> {
        let keys = self.list_keys().await?;
        if keys.iter().any(|k| k.name == WEBUI_KEY_ALIAS) {
            return Ok(());
        }
        self.create_key(
            WEBUI_KEY_ALIAS,
            None,
            None,
            None,
            Some(secrets.get(crate::secrets::WEBUI_API_KEY)),
        )
        .await?;
        Ok(())
    }
}

pub fn short_id(token: &str) -> String {
    token.chars().take(12).collect()
}

fn parse_key(v: &Value) -> Option<ApiKey> {
    let token = v["token"].as_str()?.to_string();
    let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    Some(ApiKey {
        id: short_id(&token),
        name: s("key_alias").unwrap_or_else(|| "(sans nom)".into()),
        preview: s("key_name").unwrap_or_default(),
        rpm: v["rpm_limit"].as_u64(),
        tpm: v["tpm_limit"].as_u64(),
        spend: v["spend"].as_f64().unwrap_or(0.0),
        created_at: s("created_at"),
        expires: s("expires"),
        last_used: s("last_active"),
        token,
    })
}

/// Réponse de `GET /health` (contrat public).
#[derive(Debug, Deserialize, Serialize)]
pub struct Health {
    pub status: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_litellm_key() {
        let v = json!({
            "token": "88dc28d0f030c55ed4ab77ed8faf098196cb1c05df778539800c9f1243fe6b4b",
            "key_alias": "android",
            "key_name": "sk-...a1b2",
            "rpm_limit": 60,
            "tpm_limit": null,
            "spend": 0.0,
            "created_at": "2026-10-24T12:00:00Z",
            "expires": null
        });
        let k = parse_key(&v).unwrap();
        assert_eq!(k.id, "88dc28d0f030");
        assert_eq!(k.name, "android");
        assert_eq!(k.rpm, Some(60));
        assert_eq!(k.tpm, None);
        assert_eq!(k.expires, None);
    }
}
