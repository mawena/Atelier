//! Mise à jour du binaire depuis les versions publiées sur GitHub.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::error::Fail;
use crate::gateway::http_client;
use crate::output;
use crate::{APP, VERSION};

/// Dépôt des versions publiées (surchargeable par ATELIER_REPO).
pub const DEFAULT_REPO: &str = "mawena/atelier";

pub fn repo() -> String {
    std::env::var("ATELIER_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

/// Nom de l'archive publiée pour cette architecture.
pub fn asset_name() -> String {
    format!("{APP}-{}-unknown-linux-musl", std::env::consts::ARCH)
}

/// Télécharge et installe la version demandée (ou la dernière).
/// Renvoie le chemin du binaire remplacé, ou `None` s'il était déjà à jour.
pub async fn run(to: Option<&str>) -> anyhow::Result<Option<PathBuf>> {
    let repo = repo();
    let http = http_client(Duration::from_secs(300));
    // Miroir des fichiers publiés (ATELIER_BASE_URL) : la version doit être donnée.
    let mirror = std::env::var("ATELIER_BASE_URL")
        .ok()
        .filter(|u| !u.is_empty());
    let version = match (to, &mirror) {
        (Some(v), _) => v.trim_start_matches('v').to_string(),
        (None, Some(_)) => {
            return Err(crate::error::usage("ATELIER_BASE_URL exige --to <version>"));
        }
        (None, None) => latest_version(&http, &repo).await?,
    };
    if version == VERSION && to.is_none() {
        output::success(format!("CLI déjà à jour ({VERSION})"));
        return Ok(None);
    }
    output::step(format!("téléchargement de {APP} {version}"));
    let base = mirror
        .map(|m| m.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("https://github.com/{repo}/releases/download/v{version}"));
    let asset = asset_name();
    let binary = download(&http, &format!("{base}/{asset}")).await?;
    let sums = String::from_utf8(download(&http, &format!("{base}/SHA256SUMS")).await?)
        .context("SHA256SUMS illisible")?;
    let expected =
        expected_sum(&sums, &asset).with_context(|| format!("{asset} absent de SHA256SUMS"))?;
    let actual = hex::encode(Sha256::digest(&binary));
    if actual != expected {
        anyhow::bail!(
            "somme de contrôle invalide pour {asset} : attendu {expected}, obtenu {actual}"
        );
    }

    let exe = std::env::current_exe().context("emplacement du binaire")?;
    let tmp = exe.with_extension("new");
    crate::config::write_atomic(&tmp, &binary, 0o755).map_err(|e| {
        Fail::Permission(format!(
            "impossible d'écrire {} ({e:#}) — relancez avec sudo",
            tmp.display()
        ))
    })?;
    std::fs::rename(&tmp, &exe).with_context(|| format!("remplacement de {}", exe.display()))?;
    output::success(format!("{APP} mis à jour : {VERSION} → {version}"));
    Ok(Some(exe))
}

async fn latest_version(http: &reqwest::Client, repo: &str) -> anyhow::Result<String> {
    let v: serde_json::Value = http
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("recherche de la dernière version")?
        .error_for_status()
        .context("recherche de la dernière version")?
        .json()
        .await?;
    Ok(v["tag_name"]
        .as_str()
        .context("réponse sans tag_name")?
        .trim_start_matches('v')
        .to_string())
}

async fn download(http: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let resp = http
        .get(url)
        .send()
        .await
        .with_context(|| format!("téléchargement de {url}"))?;
    let resp = resp
        .error_for_status()
        .with_context(|| format!("téléchargement de {url}"))?;
    Ok(resp.bytes().await?.to_vec())
}

pub fn expected_sum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let sum = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset).then(|| sum.to_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_parsing() {
        let sums = "abc123  atelier-x86_64-unknown-linux-musl\nDEF456 *atelier-aarch64-unknown-linux-musl\n";
        assert_eq!(
            expected_sum(sums, "atelier-x86_64-unknown-linux-musl").unwrap(),
            "abc123"
        );
        assert_eq!(
            expected_sum(sums, "atelier-aarch64-unknown-linux-musl").unwrap(),
            "def456"
        );
        assert!(expected_sum(sums, "autre").is_none());
    }
}
