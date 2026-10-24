use serde::Serialize;

use super::Ctx;
use crate::config::Config;
use crate::docker::{Container, Docker};
use crate::engine::{LoadedModel, Ollama};
use crate::output::{self, bold, dim, green, kv, red, yellow};
use crate::render;
use crate::system::{self, SystemInfo};

#[derive(Serialize)]
struct ServiceStatus {
    #[serde(flatten)]
    container: Container,
    memory: Option<u64>,
}

#[derive(Serialize)]
struct Status {
    running: bool,
    healthy: bool,
    url: String,
    admin_url: String,
    webui_url: Option<String>,
    exposed: bool,
    accelerator: &'static str,
    gpus: Vec<system::Gpu>,
    ram_total: u64,
    ram_used: u64,
    vram_total: u64,
    vram_used: u64,
    default_model: String,
    embedding_model: String,
    models_installed: Vec<String>,
    models_loaded: Vec<LoadedModel>,
    services: Vec<ServiceStatus>,
}

pub async fn run(ctx: &Ctx) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;
    let sys = SystemInfo::detect(&ctx.paths.data_dir);
    let gpu = render::gpu_enabled(&cfg, &sys);

    let mut services = Vec::new();
    for c in docker.containers().await? {
        let memory = if c.running() {
            docker.memory_usage(&c.id).await
        } else {
            None
        };
        services.push(ServiceStatus {
            container: c,
            memory,
        });
    }
    let loaded = match Ollama::connect(&docker).await {
        Ok(o) => o.loaded().await.unwrap_or_default(),
        Err(_) => vec![],
    };
    let running = !services.is_empty() && services.iter().all(|s| s.container.running());
    let healthy = running && services.iter().all(|s| s.container.healthy());
    let status = Status {
        running,
        healthy,
        url: cfg.public_url(),
        admin_url: cfg.admin_url(),
        webui_url: cfg.webui_url(),
        exposed: cfg.is_exposed(),
        accelerator: if gpu { "gpu" } else { "cpu" },
        gpus: sys.gpus.clone(),
        ram_total: sys.ram_total,
        ram_used: sys.ram_total.saturating_sub(sys.ram_available),
        vram_total: sys.vram_total(),
        vram_used: sys.vram_used(),
        default_model: cfg.engine.default_model.clone(),
        embedding_model: cfg.engine.embedding_model.clone(),
        models_installed: render::load_models(&ctx.paths)
            .into_iter()
            .map(|m| m.name)
            .collect(),
        models_loaded: loaded,
        services,
    };

    if output::json_mode() {
        return output::print_json(&status);
    }

    let state = if status.services.is_empty() {
        red("arrêtée")
    } else if status.healthy {
        green("en fonctionnement")
    } else if status.running {
        yellow("démarrée, en attente de santé")
    } else {
        red("partiellement arrêtée")
    };
    output::line(format!("{} {}", bold("Pile :"), state));
    kv("API", &status.url);
    if let Some(w) = &status.webui_url {
        kv("Interface web", w);
    }
    kv(
        "Exposition",
        if status.exposed {
            yellow("réseau")
        } else {
            "locale (127.0.0.1)".into()
        },
    );
    kv(
        "Accélération",
        if gpu {
            sys.gpus
                .iter()
                .map(|g| g.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            format!("CPU ({} cœurs)", sys.cpus)
        },
    );
    kv(
        "RAM",
        format!(
            "{} / {}",
            output::bytes(status.ram_used),
            output::bytes(status.ram_total)
        ),
    );
    if status.vram_total > 0 {
        kv(
            "VRAM",
            format!(
                "{} / {}",
                output::bytes(status.vram_used),
                output::bytes(status.vram_total)
            ),
        );
    }
    kv(
        "Modèle par défaut",
        if status.default_model.is_empty() {
            "(aucun)"
        } else {
            &status.default_model
        },
    );
    kv(
        "Modèles installés",
        status.models_installed.len().to_string(),
    );

    output::line("");
    let rows = status
        .services
        .iter()
        .map(|s| {
            let c = &s.container;
            let st = match (c.state.as_str(), c.health.as_deref()) {
                ("running", Some("healthy")) | ("running", None) => green("● actif"),
                ("running", Some("starting")) => yellow("● démarrage"),
                ("running", Some("unhealthy")) => red("● en échec"),
                ("running", Some(h)) => red(&format!("● {h}")),
                ("exited", _) => red("○ arrêté"),
                ("restarting", _) => red("○ redémarre en boucle"),
                (other, _) => red(&format!("○ {other}")),
            };
            vec![
                c.service.clone(),
                st,
                s.memory.map(output::bytes).unwrap_or_else(|| "-".into()),
                if c.ports.is_empty() {
                    dim("interne")
                } else {
                    c.ports.join(", ")
                },
            ]
        })
        .collect();
    output::table(&["SERVICE", "ÉTAT", "MÉMOIRE", "PORTS"], rows);

    output::line("");
    if status.models_loaded.is_empty() {
        output::line(dim(
            "Aucun modèle chargé en mémoire (chargement à la première requête).",
        ));
    } else {
        let rows = status
            .models_loaded
            .iter()
            .map(|m| {
                vec![
                    m.name.clone(),
                    m.size.clone(),
                    m.processor.clone(),
                    m.until.clone(),
                ]
            })
            .collect();
        output::table(&["MODÈLE CHARGÉ", "TAILLE", "PROCESSEUR", "JUSQU'À"], rows);
    }
    Ok(())
}
