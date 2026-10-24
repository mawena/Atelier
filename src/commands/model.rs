use serde_json::json;

use super::Ctx;
use crate::catalog;
use crate::cli::ModelCommand;
use crate::config::{Config, EngineKind};
use crate::docker::{self, Docker};
use crate::engine::{LocalModel, Ollama, normalize};
use crate::error::{not_found, usage};
use crate::output::{self, bold, dim, green};
use crate::render::{self, ServedModel};
use crate::system::SystemInfo;
use crate::{APP, stack};

pub async fn run(ctx: &Ctx, cmd: ModelCommand) -> anyhow::Result<()> {
    match cmd {
        ModelCommand::List => list(ctx).await,
        ModelCommand::Search { term } => search(ctx, &term),
        ModelCommand::Pull { name, default } => pull(ctx, &name, default).await,
        ModelCommand::Remove { name, force } => remove(ctx, &name, force).await,
        ModelCommand::Default { name } => set_default(ctx, &name).await,
        ModelCommand::Recommend => recommend(ctx),
    }
}

/// Un nom de dépôt Hugging Face (organisation/modèle) servi par vLLM.
fn is_vllm_name(cfg: &Config, name: &str) -> bool {
    cfg.engine.kind == EngineKind::Vllm && name.contains('/') && !name.starts_with("hf.co/")
}

fn same(a: &str, b: &str) -> bool {
    a == b || normalize(a) == normalize(b)
}

async fn list(ctx: &Ctx) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;
    let mut models: Vec<LocalModel> = match Ollama::connect(&docker).await {
        Ok(o) => o.list().await?,
        Err(_) => {
            output::warn("moteur arrêté : liste issue de la dernière synchronisation");
            render::load_models(&ctx.paths)
                .into_iter()
                .filter(|m| !m.vllm)
                .map(|m| LocalModel {
                    name: m.name,
                    id: String::new(),
                    size: String::new(),
                    modified: String::new(),
                    embedding: m.embedding,
                })
                .collect()
        }
    };
    if cfg.engine.kind == EngineKind::Vllm && !cfg.engine.vllm_model.is_empty() {
        models.push(LocalModel {
            name: cfg.engine.vllm_model.clone(),
            id: "vllm".into(),
            size: String::new(),
            modified: String::new(),
            embedding: false,
        });
    }
    if output::json_mode() {
        let items: Vec<_> = models
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "type": if m.embedding { "embedding" } else { "chat" },
                    "engine": if m.id == "vllm" { "vllm" } else { "ollama" },
                    "size": m.size,
                    "modified": m.modified,
                    "default": same(&m.name, &cfg.engine.default_model) || same(&m.name, &cfg.engine.embedding_model),
                })
            })
            .collect();
        return output::print_json(&items);
    }
    if models.is_empty() {
        output::line(format!(
            "Aucun modèle installé. Essayez « {APP} model recommend »."
        ));
        return Ok(());
    }
    let rows = models
        .iter()
        .map(|m| {
            let mark = if same(&m.name, &cfg.engine.default_model) {
                green("default")
            } else if same(&m.name, &cfg.engine.embedding_model) {
                green("default-embedding")
            } else {
                String::new()
            };
            vec![
                m.name.clone(),
                if m.embedding {
                    "embeddings".into()
                } else {
                    "chat".into()
                },
                m.size.clone(),
                m.modified.clone(),
                mark,
            ]
        })
        .collect();
    output::table(&["NOM", "TYPE", "TAILLE", "MODIFIÉ", "ALIAS"], rows);
    Ok(())
}

fn search(ctx: &Ctx, term: &str) -> anyhow::Result<()> {
    let installed: Vec<String> = render::load_models(&ctx.paths)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let results = catalog::search(term);
    if output::json_mode() {
        let items: Vec<_> = results
            .iter()
            .map(|e| {
                let mut v = serde_json::to_value(e).unwrap_or_default();
                v["installed"] = json!(installed.iter().any(|i| same(i, e.name)));
                v
            })
            .collect();
        return output::print_json(&items);
    }
    if results.is_empty() {
        output::line(format!(
            "Aucun modèle du catalogue ne correspond à « {term} ». Tout modèle de la bibliothèque Ollama \
             (https://ollama.com/library) peut aussi être installé : « {APP} model pull <nom> »."
        ));
        return Ok(());
    }
    let rows = results
        .iter()
        .map(|e| {
            vec![
                e.name.to_string(),
                e.kind.label().to_string(),
                format!("{:.1} Go", e.download_gb),
                format!("{:.0} Go", e.min_memory_gb),
                e.description.to_string(),
                if installed.iter().any(|i| same(i, e.name)) {
                    green("installé")
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    output::table(
        &[
            "NOM",
            "TYPE",
            "TÉLÉCHARGEMENT",
            "MÉMOIRE MIN",
            "DESCRIPTION",
            "",
        ],
        rows,
    );
    output::line(dim(
        "Tout modèle de https://ollama.com/library peut aussi être installé.",
    ));
    Ok(())
}

async fn pull(ctx: &Ctx, name: &str, make_default: bool) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;

    let embedding;
    if is_vllm_name(&cfg, name) {
        embedding = false;
        if cfg.engine.vllm_model != name {
            output::step(format!(
                "vLLM servira désormais {name} (téléchargement au démarrage du service)"
            ));
            cfg.engine.vllm_model = name.to_string();
            cfg.save(&ctx.paths)?;
            render::regenerate(&cfg, &ctx.paths)?;
            docker::compose(&ctx.paths, &["up", "-d", "--remove-orphans"]).await?;
        } else {
            output::success(format!("{name} est déjà le modèle servi par vLLM"));
        }
    } else {
        let ollama = Ollama::connect(&docker).await?;
        if ollama.has(name).await? {
            output::success(format!("{name} est déjà installé"));
        } else {
            ollama.pull(name).await?;
            output::success(format!("{name} téléchargé"));
        }
        embedding = ollama.is_embedding(name).await;
    }

    if embedding {
        if make_default || cfg.engine.embedding_model.is_empty() {
            cfg.engine.embedding_model = name.to_string();
        }
    } else if make_default || cfg.engine.default_model.is_empty() {
        cfg.engine.default_model = name.to_string();
    }
    cfg.save(&ctx.paths)?;
    stack::sync_models(&ctx.paths, &cfg, &docker).await?;

    let alias = if same(name, &cfg.engine.default_model) {
        Some("default")
    } else if same(name, &cfg.engine.embedding_model) {
        Some("default-embedding")
    } else {
        None
    };
    if output::json_mode() {
        return output::print_json(&json!({
            "ok": true,
            "model": name,
            "type": if embedding { "embedding" } else { "chat" },
            "alias": alias,
        }));
    }
    output::success(format!(
        "{name} est disponible via l'API{}",
        alias
            .map(|a| format!(" (alias « {a} »)"))
            .unwrap_or_default()
    ));
    Ok(())
}

async fn remove(ctx: &Ctx, name: &str, force: bool) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;
    let is_default = same(name, &cfg.engine.default_model);
    if is_default && !force {
        return Err(usage(format!(
            "{name} est le modèle par défaut : choisissez-en un autre (« {APP} model default <nom> ») ou utilisez --force"
        )));
    }
    if cfg.engine.kind == EngineKind::Vllm && cfg.engine.vllm_model == name {
        cfg.engine.vllm_model.clear();
    } else {
        let ollama = Ollama::connect(&docker).await?;
        if !ollama.has(name).await? {
            return Err(not_found(format!("le modèle {name} n'est pas installé")));
        }
        ollama.remove(name).await?;
    }
    if is_default {
        cfg.engine.default_model.clear();
    }
    if same(name, &cfg.engine.embedding_model) {
        cfg.engine.embedding_model.clear();
    }
    cfg.save(&ctx.paths)?;
    stack::sync_models(&ctx.paths, &cfg, &docker).await?;
    if output::json_mode() {
        return output::print_json(&json!({ "ok": true, "removed": name }));
    }
    output::success(format!("{name} supprimé"));
    if is_default {
        output::warn(format!(
            "plus de modèle par défaut : « {APP} model default <nom> »"
        ));
    }
    Ok(())
}

async fn set_default(ctx: &Ctx, name: &str) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;
    let models: Vec<ServedModel> = render::load_models(&ctx.paths);
    let Some(model) = models.iter().find(|m| same(&m.name, name)) else {
        return Err(not_found(format!(
            "le modèle {name} n'est pas installé — « {APP} model pull {name} »"
        )));
    };
    let alias = if model.embedding {
        cfg.engine.embedding_model = model.name.clone();
        "default-embedding"
    } else {
        cfg.engine.default_model = model.name.clone();
        "default"
    };
    cfg.save(&ctx.paths)?;
    stack::sync_models(&ctx.paths, &cfg, &docker).await?;
    if output::json_mode() {
        return output::print_json(&json!({ "ok": true, "model": model.name, "alias": alias }));
    }
    output::success(format!("« {alias} » pointe désormais vers {}", model.name));
    Ok(())
}

fn recommend(ctx: &Ctx) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths).unwrap_or_default();
    let sys = SystemInfo::detect(&ctx.paths.data_dir);
    let gpu = render::gpu_enabled(&cfg, &sys) || (!ctx.paths.is_installed() && sys.has_gpu());
    let reco = catalog::recommend(&sys, gpu);
    if output::json_mode() {
        return output::print_json(&reco);
    }
    output::kv(
        "Mémoire utilisable",
        format!(
            "{:.1} Go de {}",
            reco.budget_gb,
            if reco.basis == "gpu" { "VRAM" } else { "RAM" }
        ),
    );
    output::kv(
        "Chat",
        format!("{} — {}", bold(reco.chat.name), reco.chat.description),
    );
    output::kv(
        "Embeddings",
        format!(
            "{} — {}",
            bold(reco.embedding.name),
            reco.embedding.description
        ),
    );
    if reco.basis == "cpu" {
        output::line(dim(
            "Sans GPU, les modèles au-delà de 8B fonctionnent mais répondent lentement.",
        ));
    }
    if !reco.alternatives.is_empty() {
        output::line("");
        let rows = reco
            .alternatives
            .iter()
            .map(|e| {
                vec![
                    e.name.to_string(),
                    e.kind.label().to_string(),
                    format!("{:.1} Go", e.download_gb),
                    e.description.to_string(),
                ]
            })
            .collect();
        output::table(
            &[
                "AUTRES MODÈLES COMPATIBLES",
                "TYPE",
                "TÉLÉCHARGEMENT",
                "DESCRIPTION",
            ],
            rows,
        );
    }
    output::line("");
    output::line(format!(
        "Installer : « sudo {APP} model pull {} --default »",
        reco.chat.name
    ));
    Ok(())
}
