//! start, stop, restart, logs, uninstall.

use serde_json::json;

use super::Ctx;
use crate::config::Config;
use crate::docker::{self, Docker, SERVICES};
use crate::error::{Fail, not_found};
use crate::output;
use crate::render;
use crate::{APP, stack};

fn done(msg: &str) -> anyhow::Result<()> {
    if output::json_mode() {
        return output::print_json(&json!({ "ok": true, "message": msg }));
    }
    output::success(msg);
    Ok(())
}

pub async fn start(ctx: &Ctx) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let cfg = Config::load(&ctx.paths)?;
    Docker::connect().await?;
    let changes = render::regenerate(&cfg, &ctx.paths)?;
    stack::apply(&ctx.paths, &cfg, &changes).await?;
    done(&format!("pile démarrée — API sur {}", cfg.public_url()))
}

pub async fn stop(ctx: &Ctx) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    ctx.paths.require_installed()?;
    Docker::connect().await?;
    output::step("arrêt des services");
    docker::compose(&ctx.paths, &["stop"]).await?;
    done("pile arrêtée (données conservées)")
}

pub async fn restart(ctx: &Ctx, service: Option<String>) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;
    match service {
        Some(s) => {
            check_service(&s)?;
            if docker.container(&s).await?.is_none() {
                return Err(not_found(format!(
                    "le service {s} n'existe pas dans cette installation"
                )));
            }
            output::step(format!("redémarrage de {s}"));
            docker::compose_quiet(&ctx.paths, &["restart", &s]).await?;
            if s == "litellm" || s == "postgres" {
                stack::wait_ready(&ctx.paths, &cfg).await?;
            }
            done(&format!("{s} redémarré"))
        }
        None => {
            render::regenerate(&cfg, &ctx.paths)?;
            output::step("redémarrage des services");
            docker::compose(&ctx.paths, &["up", "-d", "--remove-orphans"]).await?;
            docker::compose_quiet(&ctx.paths, &["restart"]).await?;
            stack::wait_ready(&ctx.paths, &cfg).await?;
            done("pile redémarrée")
        }
    }
}

fn check_service(s: &str) -> anyhow::Result<()> {
    if SERVICES.contains(&s) {
        Ok(())
    } else {
        Err(Fail::Usage(format!(
            "service inconnu « {s} » (services : {})",
            SERVICES.join(", ")
        ))
        .into())
    }
}

pub async fn logs(
    ctx: &Ctx,
    service: Option<String>,
    follow: bool,
    tail: usize,
) -> anyhow::Result<()> {
    ctx.paths.require_installed()?;
    let docker = Docker::connect().await?;
    let all = docker.containers().await?;
    let selected: Vec<_> = match &service {
        Some(s) => {
            check_service(s)?;
            all.into_iter().filter(|c| &c.service == s).collect()
        }
        None => all,
    };
    if selected.is_empty() {
        return Err(not_found(match service {
            Some(s) => format!("aucun conteneur pour le service {s}"),
            None => {
                format!("aucun conteneur — la pile est-elle installée ? (« sudo {APP} start »)")
            }
        }));
    }
    docker.logs(&selected, follow, tail).await
}

pub async fn uninstall(ctx: &Ctx, purge: bool, images: bool) -> anyhow::Result<()> {
    let paths = &ctx.paths;
    if !paths.is_installed() && !paths.data_dir.exists() {
        return done(&format!("{APP} n'est pas installé : rien à faire"));
    }
    paths.require_privileges()?;
    if purge {
        output::warn(format!(
            "--purge supprime définitivement les modèles, les clés API, la base et la configuration ({} et {})",
            paths.config_dir.display(),
            paths.data_dir.display()
        ));
        if !output::confirm("Tout supprimer ?", false)? {
            return Err(Fail::Cancelled.into());
        }
    }
    let cfg = Config::load(paths).ok();
    let docker = Docker::connect().await;
    if docker.is_ok() && paths.compose_file().exists() {
        output::step("suppression des conteneurs");
        docker::compose(paths, &["down", "--remove-orphans"]).await?;
    }
    if images && let (Ok(_), Some(cfg)) = (&docker, &cfg) {
        output::step("suppression des images");
        let i = &cfg.images;
        for image in [
            &i.caddy,
            &i.litellm,
            &i.postgres,
            &i.ollama,
            &i.vllm,
            &i.webui,
        ] {
            let _ = tokio::process::Command::new("docker")
                .args(["image", "rm", image])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
    }
    if purge {
        for dir in [&paths.data_dir, &paths.config_dir] {
            if dir.exists() {
                output::step(format!("suppression de {}", dir.display()));
                if let Err(e) = std::fs::remove_dir_all(dir) {
                    // Fichiers créés par les conteneurs sous un autre uid (sans root).
                    let image = cfg
                        .as_ref()
                        .map(|c| c.images.postgres.clone())
                        .unwrap_or_default();
                    if docker.is_err()
                        || image.is_empty()
                        || !remove_with_container(dir, &image).await
                    {
                        return Err(anyhow::Error::from(e)
                            .context(format!("suppression de {}", dir.display())));
                    }
                }
            }
        }
        return done(&format!(
            "{APP} est entièrement supprimé (le binaire reste : {})",
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        ));
    }
    done(&format!(
        "pile supprimée ; modèles, clés et configuration conservés ({}). « sudo {APP} install » pour réinstaller, --purge pour tout effacer",
        paths.data_dir.display()
    ))
}

/// Supprime un dossier via un conteneur jetable (root dans le conteneur).
async fn remove_with_container(dir: &std::path::Path, image: &str) -> bool {
    let mount = format!("{}:/cible", dir.display());
    let ok = tokio::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--entrypoint",
            "sh",
            "-v",
            &mount,
            image,
            "-c",
            "rm -rf /cible/* /cible/.[!.]*",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    ok && std::fs::remove_dir_all(dir).is_ok()
}
