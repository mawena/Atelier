use serde_json::json;

use super::Ctx;
use crate::config::{Config, Images};
use crate::docker::{self, Docker};
use crate::output;
use crate::{render, stack};

pub async fn run(
    ctx: &Ctx,
    images_only: bool,
    cli_only: bool,
    to: Option<String>,
) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cli_updated = None;
    if !images_only {
        cli_updated = super::selfupdate::run(to.as_deref()).await?;
        if let Some(new_exe) = &cli_updated
            && !cli_only
        {
            // Le nouveau binaire connaît les nouvelles versions d'images.
            output::step("poursuite de la mise à jour avec le nouveau CLI");
            let mut cmd = tokio::process::Command::new(new_exe);
            cmd.args(["update", "--images-only"]);
            if output::json_mode() {
                cmd.arg("--json");
            }
            if output::assume_yes() {
                cmd.arg("--yes");
            }
            let status = cmd.status().await?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
    if cli_only {
        if output::json_mode() {
            return output::print_json(
                &json!({ "ok": true, "cli_updated": cli_updated.is_some() }),
            );
        }
        return Ok(());
    }
    update_images(ctx).await
}

async fn update_images(ctx: &Ctx) -> anyhow::Result<()> {
    let mut cfg = Config::load(&ctx.paths)?;
    Docker::connect().await?;
    let bumped = bump_images(&mut cfg.images, &Images::default());
    for (old, new) in &bumped {
        output::step(format!("{old} → {new}"));
    }
    cfg.save(&ctx.paths)?;
    let changes = render::regenerate(&cfg, &ctx.paths)?;
    output::step("téléchargement des images");
    docker::compose(&ctx.paths, &["pull"]).await?;
    stack::apply(&ctx.paths, &cfg, &changes).await?;
    if output::json_mode() {
        return output::print_json(&json!({
            "ok": true,
            "images": bumped.iter().map(|(o, n)| json!({ "from": o, "to": n })).collect::<Vec<_>>(),
        }));
    }
    output::success("images à jour, services redémarrés (données conservées)");
    Ok(())
}

/// Passe à la version épinglée par ce CLI les images dont le dépôt est celui
/// par défaut. Les images personnalisées (autre dépôt) ne sont pas touchées.
pub fn bump_images(current: &mut Images, defaults: &Images) -> Vec<(String, String)> {
    let mut changed = Vec::new();
    let pairs = [
        (&mut current.ollama, &defaults.ollama),
        (&mut current.vllm, &defaults.vllm),
        (&mut current.litellm, &defaults.litellm),
        (&mut current.postgres, &defaults.postgres),
        (&mut current.caddy, &defaults.caddy),
        (&mut current.webui, &defaults.webui),
    ];
    for (cur, def) in pairs {
        if repository(cur) == repository(def) && cur != def {
            changed.push((cur.clone(), def.clone()));
            *cur = def.clone();
        }
    }
    changed
}

fn repository(image: &str) -> &str {
    match image.rfind(':') {
        Some(i) if !image[i..].contains('/') => &image[..i],
        _ => image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_only_known_repositories() {
        let defaults = Images::default();
        let mut cur = Images {
            ollama: "ollama/ollama:0.1.0".into(),
            caddy: "registre.local/caddy:perso".into(),
            ..Default::default()
        };
        let changed = bump_images(&mut cur, &defaults);
        assert_eq!(changed.len(), 1);
        assert_eq!(cur.ollama, defaults.ollama);
        assert_eq!(cur.caddy, "registre.local/caddy:perso");
    }

    #[test]
    fn repository_parsing() {
        assert_eq!(repository("ghcr.io/a/b:v1"), "ghcr.io/a/b");
        assert_eq!(repository("localhost:5000/img"), "localhost:5000/img");
        assert_eq!(repository("caddy"), "caddy");
    }
}
