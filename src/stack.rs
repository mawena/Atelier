//! Opérations sur la pile complète : appliquer la configuration, attendre la
//! santé, synchroniser les modèles avec la passerelle.

use std::time::Duration;

use crate::config::{Config, EngineKind};
use crate::docker::{self, Docker};
use crate::engine::Ollama;
use crate::gateway::Gateway;
use crate::output;
use crate::paths::Paths;
use crate::render::{self, Changes, ServedModel};
use crate::secrets::Secrets;

pub const READY_TIMEOUT: Duration = Duration::from_secs(240);

/// Démarre ou met à jour les conteneurs, redémarre ceux dont un fichier a
/// changé, puis attend que la passerelle soit prête.
pub async fn apply(paths: &Paths, cfg: &Config, changes: &Changes) -> anyhow::Result<()> {
    output::step("démarrage des services");
    docker::compose(paths, &["up", "-d", "--remove-orphans"]).await?;
    let mut restart = Vec::new();
    if changes.litellm {
        restart.push("litellm");
    }
    if changes.caddy {
        restart.push("caddy");
    }
    if !restart.is_empty() {
        let mut args = vec!["restart"];
        args.extend(restart);
        docker::compose_quiet(paths, &args).await?;
    }
    wait_ready(paths, cfg).await
}

/// Attend la passerelle puis s'assure que la clé de l'interface web existe.
pub async fn wait_ready(paths: &Paths, cfg: &Config) -> anyhow::Result<()> {
    let secrets = Secrets::load(paths)?;
    let gw = Gateway::new(cfg, &secrets);
    let bar = spinner("attente de la passerelle…");
    let res = gw.wait_ready(READY_TIMEOUT).await;
    bar.finish_and_clear();
    res?;
    if cfg.webui.enabled {
        gw.ensure_webui_key(&secrets).await?;
    }
    Ok(())
}

pub fn spinner(msg: &str) -> indicatif::ProgressBar {
    let bar = indicatif::ProgressBar::new_spinner();
    if output::json_mode() || !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }
    bar.enable_steady_tick(Duration::from_millis(120));
    bar.set_message(msg.to_string());
    bar
}

/// Relit les modèles du moteur, met à jour la passerelle et la redémarre si
/// la liste a changé.
pub async fn sync_models(
    paths: &Paths,
    cfg: &Config,
    docker: &Docker,
) -> anyhow::Result<Vec<ServedModel>> {
    let ollama = Ollama::connect(docker).await?;
    let mut models: Vec<ServedModel> = ollama
        .list()
        .await?
        .into_iter()
        .map(|m| ServedModel {
            name: m.name,
            embedding: m.embedding,
            vllm: false,
        })
        .collect();
    if cfg.engine.kind == EngineKind::Vllm && !cfg.engine.vllm_model.is_empty() {
        models.push(ServedModel {
            name: cfg.engine.vllm_model.clone(),
            embedding: false,
            vllm: true,
        });
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    render::save_models(paths, &models)?;
    let changes = render::regenerate(cfg, paths)?;
    if changes.compose {
        docker::compose(paths, &["up", "-d", "--remove-orphans"]).await?;
    }
    if changes.litellm {
        output::step("mise à jour de la passerelle");
        docker::compose_quiet(paths, &["restart", "litellm"]).await?;
        wait_ready(paths, cfg).await?;
    }
    Ok(models)
}
