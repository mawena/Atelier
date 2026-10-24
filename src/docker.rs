//! Accès à Docker : API via bollard, orchestration via `docker compose`.

use std::collections::HashMap;
use std::process::Stdio;

use anyhow::Context;
use bollard::exec::StartExecResults;
use bollard::models::ExecConfig;
use bollard::query_parameters::{
    ListContainersOptionsBuilder, LogsOptionsBuilder, StatsOptionsBuilder,
};
use futures_util::{StreamExt, stream};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::APP;
use crate::error::Fail;
use crate::paths::Paths;

/// Services connus de la pile, dans l'ordre d'affichage.
pub const SERVICES: &[&str] = &[
    "caddy",
    "litellm",
    "postgres",
    "ollama",
    "vllm",
    "open-webui",
];

pub struct Docker {
    client: bollard::Docker,
}

#[derive(Debug, Clone, Serialize)]
pub struct Container {
    pub service: String,
    pub name: String,
    pub id: String,
    pub image: String,
    pub state: String,
    pub health: Option<String>,
    pub status: String,
    pub ports: Vec<String>,
}

impl Container {
    pub fn running(&self) -> bool {
        self.state == "running"
    }
    pub fn healthy(&self) -> bool {
        self.running()
            && self
                .health
                .as_deref()
                .map(|h| h == "healthy")
                .unwrap_or(true)
    }
}

impl Docker {
    /// Connexion au démon local ; erreur explicite (code 4) s'il est absent.
    pub async fn connect() -> anyhow::Result<Self> {
        let client = bollard::Docker::connect_with_local_defaults().map_err(|e| {
            Fail::Docker(format!(
                "Docker est introuvable ({e}). Lancez « sudo {APP} install »."
            ))
        })?;
        client.ping().await.map_err(|e| {
            let hint = if e.to_string().contains("ermission") {
                "droits insuffisants sur /var/run/docker.sock — relancez avec sudo"
            } else {
                "le démon Docker ne répond pas — « sudo systemctl start docker »"
            };
            Fail::Docker(format!("Docker indisponible : {hint}"))
        })?;
        Ok(Docker { client })
    }

    pub async fn server_version(&self) -> String {
        self.client
            .version()
            .await
            .ok()
            .and_then(|v| v.version)
            .unwrap_or_else(|| "?".into())
    }

    /// Runtimes déclarés (contient « nvidia » si le toolkit est configuré).
    pub async fn runtimes(&self) -> Vec<String> {
        self.client
            .info()
            .await
            .ok()
            .and_then(|i| i.runtimes)
            .map(|r| r.into_keys().collect())
            .unwrap_or_default()
    }

    /// Conteneurs du projet, triés selon l'ordre des services.
    pub async fn containers(&self) -> anyhow::Result<Vec<Container>> {
        let mut filters = HashMap::new();
        filters.insert(
            "label".to_string(),
            vec![format!("com.docker.compose.project={APP}")],
        );
        let opts = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();
        let list = self
            .client
            .list_containers(Some(opts))
            .await
            .context("liste des conteneurs")?;
        let mut out: Vec<Container> = list
            .into_iter()
            .map(|c| {
                let labels = c.labels.unwrap_or_default();
                let enum_str = |v: serde_json::Value| v.as_str().unwrap_or_default().to_string();
                Container {
                    service: labels
                        .get("com.docker.compose.service")
                        .cloned()
                        .unwrap_or_default(),
                    name: c
                        .names
                        .and_then(|n| n.into_iter().next())
                        .unwrap_or_default()
                        .trim_start_matches('/')
                        .to_string(),
                    id: c.id.unwrap_or_default(),
                    image: c.image.unwrap_or_default(),
                    state: c
                        .state
                        .map(|s| enum_str(serde_json::to_value(s).unwrap_or_default()))
                        .unwrap_or_default(),
                    health: c
                        .health
                        .and_then(|h| h.status)
                        .map(|s| enum_str(serde_json::to_value(s).unwrap_or_default()))
                        .filter(|s| !s.is_empty() && s != "none"),
                    status: c.status.unwrap_or_default(),
                    ports: c
                        .ports
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|p| {
                            p.public_port.map(|pp| {
                                format!("{}:{}->{}", p.ip.unwrap_or_default(), pp, p.private_port)
                            })
                        })
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                }
            })
            .collect();
        out.sort_by_key(|c| {
            SERVICES
                .iter()
                .position(|s| *s == c.service)
                .unwrap_or(usize::MAX)
        });
        Ok(out)
    }

    pub async fn container(&self, service: &str) -> anyhow::Result<Option<Container>> {
        Ok(self
            .containers()
            .await?
            .into_iter()
            .find(|c| c.service == service))
    }

    /// Conteneur en cours d'exécution d'un service, ou erreur explicite.
    pub async fn running(&self, service: &str) -> anyhow::Result<Container> {
        match self.container(service).await? {
            Some(c) if c.running() => Ok(c),
            _ => Err(Fail::Unhealthy(format!(
                "le service {service} n'est pas démarré — lancez « sudo {APP} start »"
            ))
            .into()),
        }
    }

    /// Mémoire utilisée par un conteneur (octets, hors cache).
    pub async fn memory_usage(&self, id: &str) -> Option<u64> {
        let opts = StatsOptionsBuilder::default()
            .stream(false)
            .one_shot(true)
            .build();
        let stats = self.client.stats(id, Some(opts)).next().await?.ok()?;
        let mem = stats.memory_stats?;
        let usage = mem.usage?;
        let cache = mem
            .stats
            .as_ref()
            .and_then(|s| s.get("inactive_file").or_else(|| s.get("cache")).copied())
            .unwrap_or(0);
        Some(usage.saturating_sub(cache))
    }

    /// Exécute une commande dans un conteneur et renvoie (code, sortie).
    pub async fn exec(&self, container: &str, cmd: &[&str]) -> anyhow::Result<(i64, String)> {
        let mut out = String::new();
        let code = self
            .exec_with(container, cmd, None, |chunk| out.push_str(chunk))
            .await?;
        Ok((code, out))
    }

    /// Exécute une commande, transmet `input` sur stdin et chaque morceau de
    /// sortie à `on_output`. Renvoie le code de sortie.
    pub async fn exec_with(
        &self,
        container: &str,
        cmd: &[&str],
        stdin_data: Option<tokio::fs::File>,
        mut on_output: impl FnMut(&str),
    ) -> anyhow::Result<i64> {
        let exec = self
            .client
            .create_exec(
                container,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(stdin_data.is_some()),
                    cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("exécution de « {} »", cmd.join(" ")))?;
        if let StartExecResults::Attached {
            mut output,
            input: mut stdin,
        } = self.client.start_exec(&exec.id, None).await?
        {
            if let Some(mut file) = stdin_data {
                tokio::io::copy(&mut file, &mut stdin)
                    .await
                    .context("envoi des données")?;
                stdin.shutdown().await.ok();
            }
            drop(stdin);
            while let Some(chunk) = output.next().await {
                on_output(&chunk?.to_string());
            }
        }
        let inspect = self.client.inspect_exec(&exec.id).await?;
        Ok(inspect.exit_code.unwrap_or(0))
    }

    /// Exécute une commande et écrit sa sortie standard dans un fichier.
    pub async fn exec_to_file(
        &self,
        container: &str,
        cmd: &[&str],
        dest: &mut tokio::fs::File,
    ) -> anyhow::Result<()> {
        let exec = self
            .client
            .create_exec(
                container,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
                    ..Default::default()
                },
            )
            .await?;
        let mut stderr = String::new();
        if let StartExecResults::Attached { mut output, .. } =
            self.client.start_exec(&exec.id, None).await?
        {
            while let Some(chunk) = output.next().await {
                match chunk? {
                    bollard::container::LogOutput::StdOut { message } => {
                        dest.write_all(&message).await?
                    }
                    other => stderr.push_str(&other.to_string()),
                }
            }
        }
        dest.flush().await?;
        let code = self
            .client
            .inspect_exec(&exec.id)
            .await?
            .exit_code
            .unwrap_or(0);
        if code != 0 {
            anyhow::bail!(
                "« {} » a échoué ({code}) : {}",
                cmd.join(" "),
                stderr.trim()
            );
        }
        Ok(())
    }

    /// Affiche les journaux d'un ou plusieurs conteneurs, préfixés par service.
    pub async fn logs(
        &self,
        containers: &[Container],
        follow: bool,
        tail: usize,
    ) -> anyhow::Result<()> {
        let width = containers
            .iter()
            .map(|c| c.service.len())
            .max()
            .unwrap_or(0);
        let prefix_all = containers.len() > 1;
        let streams = containers.iter().map(|c| {
            let opts = LogsOptionsBuilder::default()
                .follow(follow)
                .stdout(true)
                .stderr(true)
                .tail(&tail.to_string())
                .build();
            let service = c.service.clone();
            self.client
                .logs(&c.id, Some(opts))
                .map(move |r| (service.clone(), r))
        });
        let mut merged = stream::select_all(streams.map(Box::pin));
        let mut stdout = tokio::io::stdout();
        let mut pending: HashMap<String, String> = HashMap::new();
        while let Some((service, item)) = merged.next().await {
            let text = item.context("lecture des journaux")?.to_string();
            let buf = pending.entry(service.clone()).or_default();
            buf.push_str(&text);
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let out = if prefix_all {
                    format!(
                        "{} {}",
                        crate::output::cyan(&format!("{service:<width$} |")),
                        line
                    )
                } else {
                    line
                };
                stdout.write_all(out.as_bytes()).await?;
            }
        }
        for (service, rest) in pending {
            if !rest.is_empty() {
                stdout
                    .write_all(format!("{service} | {rest}\n").as_bytes())
                    .await?;
            }
        }
        stdout.flush().await?;
        Ok(())
    }
}

/// Vrai si `docker compose` (plugin v2) est disponible.
pub fn compose_available() -> bool {
    std::process::Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn compose_cmd(paths: &Paths, args: &[&str]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("compose")
        .arg("--project-name")
        .arg(APP)
        .arg("--project-directory")
        .arg(&paths.config_dir)
        .arg("--env-file")
        .arg(paths.secrets_file())
        .arg("-f")
        .arg(paths.compose_file())
        .args(args);
    cmd
}

/// Lance `docker compose …` ; la sortie est affichée (sur stderr en mode JSON).
pub async fn compose(paths: &Paths, args: &[&str]) -> anyhow::Result<()> {
    let mut cmd = compose_cmd(paths, args);
    if crate::output::json_mode() {
        cmd.stdout(Stdio::null());
    }
    let status = cmd
        .status()
        .await
        .map_err(|e| Fail::Docker(format!("impossible de lancer docker compose : {e}")))?;
    if !status.success() {
        return Err(Fail::Docker(format!("« docker compose {} » a échoué", args.join(" "))).into());
    }
    Ok(())
}

/// Lance `docker compose …` en silence et renvoie la sortie en cas d'échec.
pub async fn compose_quiet(paths: &Paths, args: &[&str]) -> anyhow::Result<()> {
    let out = compose_cmd(paths, args)
        .output()
        .await
        .map_err(|e| Fail::Docker(format!("impossible de lancer docker compose : {e}")))?;
    if !out.status.success() {
        return Err(Fail::Docker(format!(
            "« docker compose {} » a échoué : {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
        .into());
    }
    Ok(())
}
