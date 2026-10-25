//! Opérations sur le moteur d'inférence (Ollama, via `docker exec`).
//!
//! Le moteur n'est jamais publié sur l'hôte : le CLI passe par le binaire
//! `ollama` présent dans le conteneur.

use serde::Serialize;

use crate::catalog;
use crate::docker::Docker;
use crate::output;

#[derive(Debug, Clone, Serialize)]
pub struct LocalModel {
    pub name: String,
    pub id: String,
    pub size: String,
    pub modified: String,
    pub embedding: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedModel {
    pub name: String,
    pub size: String,
    pub processor: String,
    pub until: String,
}

pub struct Ollama<'a> {
    docker: &'a Docker,
    container: String,
}

impl<'a> Ollama<'a> {
    pub async fn connect(docker: &'a Docker) -> anyhow::Result<Self> {
        let c = docker.running("ollama").await?;
        Ok(Ollama {
            docker,
            container: c.id,
        })
    }

    async fn run(&self, args: &[&str]) -> anyhow::Result<String> {
        let mut cmd = vec!["ollama"];
        cmd.extend_from_slice(args);
        let (code, out) = self.docker.exec(&self.container, &cmd).await?;
        if code != 0 {
            anyhow::bail!(
                "« ollama {} » a échoué : {}",
                args.join(" "),
                strip_ansi(&out).trim()
            );
        }
        Ok(out)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<LocalModel>> {
        let out = self.run(&["list"]).await?;
        let mut models = Vec::new();
        for row in parse_table(&out) {
            let name = row.get("NAME").cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let embedding = self.is_embedding(&name).await;
            models.push(LocalModel {
                id: row.get("ID").cloned().unwrap_or_default(),
                size: row.get("SIZE").cloned().unwrap_or_default(),
                modified: row.get("MODIFIED").cloned().unwrap_or_default(),
                embedding,
                name,
            });
        }
        Ok(models)
    }

    /// Modèles chargés en mémoire.
    pub async fn loaded(&self) -> anyhow::Result<Vec<LoadedModel>> {
        let out = self.run(&["ps"]).await?;
        Ok(parse_table(&out)
            .into_iter()
            .filter(|r| r.get("NAME").is_some_and(|n| !n.is_empty()))
            .map(|r| LoadedModel {
                name: r.get("NAME").cloned().unwrap_or_default(),
                size: r.get("SIZE").cloned().unwrap_or_default(),
                processor: r.get("PROCESSOR").cloned().unwrap_or_default(),
                until: r.get("UNTIL").cloned().unwrap_or_default(),
            })
            .collect())
    }

    /// Capacités annoncées par `ollama show` ; repli sur le nom.
    pub async fn is_embedding(&self, name: &str) -> bool {
        match self.run(&["show", name]).await {
            Ok(out) => parse_capabilities(&out)
                .map(|caps| {
                    caps.iter().any(|c| c == "embedding") && !caps.iter().any(|c| c == "completion")
                })
                .unwrap_or_else(|| catalog::looks_like_embedding(name)),
            Err(_) => catalog::looks_like_embedding(name),
        }
    }

    pub async fn has(&self, name: &str) -> anyhow::Result<bool> {
        let wanted = normalize(name);
        self.run(&["list"]).await.map(|out| {
            parse_table(&out).iter().any(|r| {
                r.get("NAME")
                    .map(|n| normalize(n) == wanted)
                    .unwrap_or(false)
            })
        })
    }

    /// Télécharge un modèle en affichant la progression.
    pub async fn pull(&self, name: &str) -> anyhow::Result<()> {
        let bar = indicatif::ProgressBar::new_spinner();
        if output::json_mode() {
            bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        }
        bar.enable_steady_tick(std::time::Duration::from_millis(120));
        bar.set_message(format!("téléchargement de {name}…"));
        let mut last_err = String::new();
        let code = self
            .docker
            .exec_with(&self.container, &["ollama", "pull", name], None, |chunk| {
                for part in strip_ansi(chunk).split(['\r', '\n']) {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    if part.starts_with("Error") || part.contains("error") {
                        last_err = part.to_string();
                    }
                    bar.set_message(format!("{name} : {part}"));
                }
            })
            .await?;
        bar.finish_and_clear();
        if code != 0 {
            anyhow::bail!(
                "téléchargement de {name} impossible : {}",
                if last_err.is_empty() {
                    "erreur inconnue".into()
                } else {
                    last_err
                }
            );
        }
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> anyhow::Result<()> {
        self.run(&["rm", name]).await.map(|_| ())
    }
}

/// `qwen3` et `qwen3:latest` désignent le même modèle.
pub fn normalize(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{name}:latest")
    }
}

/// Retire les séquences d'échappement ANSI.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Découpe une sortie tabulaire d'Ollama selon la position des en-têtes.
pub fn parse_table(out: &str) -> Vec<std::collections::HashMap<String, String>> {
    let clean = strip_ansi(out);
    let mut lines = clean.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return vec![];
    };
    let mut cols: Vec<(String, usize)> = Vec::new();
    let bytes = header.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b' ' && (i == 0 || (i >= 2 && bytes[i - 1] == b' ' && bytes[i - 2] == b' '))
        {
            let end = header[i..]
                .find("  ")
                .map(|e| i + e)
                .unwrap_or(header.len());
            cols.push((header[i..end].trim().to_string(), i));
            i = end;
        } else {
            i += 1;
        }
    }
    lines
        .map(|line| {
            let mut row = std::collections::HashMap::new();
            for (idx, (name, start)) in cols.iter().enumerate() {
                let end = cols.get(idx + 1).map(|c| c.1).unwrap_or(usize::MAX);
                let cell = line
                    .get(*start..end.min(line.len()))
                    .or_else(|| line.get(*start..))
                    .unwrap_or("");
                row.insert(name.clone(), cell.trim().to_string());
            }
            row
        })
        .collect()
}

/// Lit la section « Capabilities » de `ollama show`.
pub fn parse_capabilities(out: &str) -> Option<Vec<String>> {
    let clean = strip_ansi(out);
    let mut lines = clean.lines().skip_while(|l| l.trim() != "Capabilities");
    lines.next()?;
    Some(
        lines
            .take_while(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_lowercase())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_table() {
        let out = "NAME                       ID              SIZE      MODIFIED\n\
                   qwen3:4b                   2bfd38a7daaf    2.6 GB    3 minutes ago\n\
                   nomic-embed-text:latest    0a109f422b47    274 MB    2 days ago\n";
        let rows = parse_table(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["NAME"], "qwen3:4b");
        assert_eq!(rows[0]["SIZE"], "2.6 GB");
        assert_eq!(rows[1]["MODIFIED"], "2 days ago");
    }

    #[test]
    fn ps_table() {
        let out = "NAME        ID              SIZE      PROCESSOR    CONTEXT    UNTIL\n\
                   qwen3:4b    2bfd38a7daaf    4.1 GB    100% GPU     4096       9 minutes from now\n";
        let rows = parse_table(out);
        assert_eq!(rows[0]["PROCESSOR"], "100% GPU");
        assert_eq!(rows[0]["UNTIL"], "9 minutes from now");
    }

    #[test]
    fn empty_table() {
        assert!(parse_table("NAME    ID    SIZE    MODIFIED\n").is_empty());
        assert!(parse_table("").is_empty());
    }

    #[test]
    fn capabilities() {
        let out = "  Model\n    architecture        nomic-bert\n\n  Capabilities\n    embedding\n\n  License\n    Apache\n";
        assert_eq!(parse_capabilities(out).unwrap(), vec!["embedding"]);
        let out = "  Capabilities\n    completion\n    tools\n    thinking\n";
        assert_eq!(
            parse_capabilities(out).unwrap(),
            vec!["completion", "tools", "thinking"]
        );
        assert!(parse_capabilities("Model\n").is_none());
    }

    #[test]
    fn ansi() {
        assert_eq!(strip_ansi("\u{1b}[?25lpulling\u{1b}[K ok"), "pulling ok");
        assert_eq!(normalize("qwen3"), "qwen3:latest");
        assert_eq!(normalize("qwen3:4b"), "qwen3:4b");
    }
}
