use anyhow::Context;
use serde_json::json;

use super::Ctx;
use crate::catalog::{self, Recommendation};
use crate::cli::{EngineArg, GpuArg, InstallArgs};
use crate::config::{Config, EngineKind, GpuMode, is_loopback};
use crate::docker::{self, Docker};
use crate::engine::Ollama;
use crate::error::Fail;
use crate::gateway::{CreatedKey, Gateway, WEBUI_KEY_ALIAS};
use crate::output::{self, bold, kv};
use crate::paths::is_root;
use crate::render;
use crate::secrets::Secrets;
use crate::system::{self, SystemInfo, which};
use crate::{APP, provision, stack};

const GB: u64 = 1024 * 1024 * 1024;

pub async fn run(ctx: &Ctx, args: InstallArgs) -> anyhow::Result<()> {
    let paths = &ctx.paths;
    if paths.system && !is_root() {
        return Err(Fail::Permission(format!(
            "l'installation nécessite les droits root : « sudo {APP} install »"
        ))
        .into());
    }
    paths.require_privileges()?;
    let first_install = !paths.is_installed();

    // 1. Machine
    output::title(format!("{APP} — installation"));
    let mut sys = SystemInfo::detect(&paths.data_dir);
    check_machine(&sys)?;

    // 2. Docker
    ensure_docker(&sys, args.skip_docker_install).await?;
    let docker = Docker::connect().await?;
    output::success(format!("Docker {} prêt", docker.server_version().await));

    // 3. Configuration
    let mut cfg = if first_install {
        Config::default()
    } else {
        Config::load(paths)?
    };
    apply_flags(&mut cfg, &args)?;

    // 4. GPU
    sys = ensure_gpu(&cfg, sys, &docker).await?;
    let gpu = render::gpu_enabled(&cfg, &sys);
    output::kv("Accélération", if gpu { "GPU NVIDIA" } else { "CPU" });

    // 5. Choix des modèles
    let reco = catalog::recommend(&sys, gpu);
    let (chat_model, embed_model) = choose_models(&cfg, &args, &reco)?;
    if let Some(m) = &chat_model {
        if cfg.engine.kind == EngineKind::Vllm && m.contains('/') && !m.starts_with("hf.co/") {
            cfg.engine.vllm_model = m.clone();
        }
        cfg.engine.default_model = m.clone();
    }
    if let Some(m) = &embed_model {
        cfg.engine.embedding_model = m.clone();
    }
    cfg.save(paths)?;
    let secrets = Secrets::ensure(paths)?;
    output::success(format!(
        "configuration écrite dans {}",
        paths.config_dir.display()
    ));

    // 6. Fichiers générés et démarrage
    let changes = render::regenerate(&cfg, paths)?;
    output::step("téléchargement des images (peut prendre plusieurs minutes)");
    docker::compose(paths, &["pull"]).await?;
    stack::apply(paths, &cfg, &changes).await?;
    output::success("services démarrés");

    // 7. Modèles
    let ollama_models: Vec<&String> = [&chat_model, &embed_model]
        .into_iter()
        .flatten()
        .filter(|m| !(cfg.engine.kind == EngineKind::Vllm && **m == cfg.engine.vllm_model))
        .collect();
    if !ollama_models.is_empty() {
        let ollama = Ollama::connect(&docker).await?;
        for m in ollama_models {
            if ollama.has(m).await? {
                output::success(format!("modèle {m} déjà présent"));
            } else {
                ollama.pull(m).await?;
                output::success(format!("modèle {m} téléchargé"));
            }
        }
    }
    if chat_model.is_some() || embed_model.is_some() {
        stack::sync_models(paths, &cfg, &docker).await?;
    }

    // 8. Première clé
    let gw = Gateway::new(&cfg, &secrets);
    let existing: Vec<_> = gw
        .list_keys()
        .await?
        .into_iter()
        .filter(|k| k.name != WEBUI_KEY_ALIAS)
        .collect();
    let created = if existing.is_empty() {
        let rpm = Some(cfg.limits.rpm);
        let tpm = Some(cfg.limits.tpm);
        Some(gw.create_key(&args.key_name, rpm, tpm, None, None).await?)
    } else {
        None
    };

    summary(&cfg, &sys, gpu, created.as_ref(), existing.len())
}

fn check_machine(sys: &SystemInfo) -> anyhow::Result<()> {
    let os = if sys.os.pretty_name.is_empty() {
        "Linux (inconnu)".to_string()
    } else {
        sys.os.pretty_name.clone()
    };
    kv("Système", format!("{os} ({})", sys.arch));
    kv("CPU", format!("{} cœurs", sys.cpus));
    kv(
        "RAM",
        format!(
            "{} (disponible : {})",
            output::bytes(sys.ram_total),
            output::bytes(sys.ram_available)
        ),
    );
    kv("Disque libre", output::bytes(sys.disk_free));
    for g in &sys.gpus {
        kv(
            "GPU",
            format!(
                "{} — {} Mo VRAM, pilote {}",
                g.name, g.vram_total_mb, g.driver
            ),
        );
    }
    if !matches!(sys.arch.as_str(), "x86_64" | "aarch64") {
        return Err(Fail::Usage(format!(
            "architecture {} non prise en charge (amd64 ou arm64 requis)",
            sys.arch
        ))
        .into());
    }
    if sys.os.family() == system::OsFamily::Other {
        output::warn("distribution non reconnue : Ubuntu/Debian sont recommandées, on continue");
    }
    if sys.ram_total < 2 * GB {
        output::warn("moins de 2 Go de RAM : seuls de très petits modèles fonctionneront");
    }
    if sys.disk_free < 5 * GB {
        return Err(Fail::Usage(format!(
            "espace disque insuffisant ({} libres, 5 Go minimum pour les images et un petit modèle)",
            output::bytes(sys.disk_free)
        ))
        .into());
    } else if sys.disk_free < 20 * GB {
        output::warn("moins de 20 Go libres : prévoyez de la place pour les modèles");
    }
    Ok(())
}

async fn ensure_docker(sys: &SystemInfo, skip: bool) -> anyhow::Result<()> {
    if !which("docker") {
        if skip {
            return Err(
                Fail::Docker("Docker n'est pas installé (--skip-docker-install)".into()).into(),
            );
        }
        if !output::confirm("Docker n'est pas installé. L'installer maintenant ?", true)? {
            return Err(Fail::Cancelled.into());
        }
        provision::install_docker(&sys.os).await?;
    } else if Docker::connect().await.is_err() && !skip {
        provision::enable_docker().await.ok();
    }
    if !docker::compose_available() {
        if skip {
            return Err(Fail::Docker("le plugin Docker Compose v2 est absent".into()).into());
        }
        provision::install_compose(&sys.os).await?;
    }
    Ok(())
}

async fn ensure_gpu(cfg: &Config, sys: SystemInfo, docker: &Docker) -> anyhow::Result<SystemInfo> {
    if cfg.engine.gpu == GpuMode::Cpu {
        return Ok(sys);
    }
    if !sys.has_gpu() {
        if system::nvidia_pci_present() {
            output::warn(
                "carte NVIDIA détectée mais pilote absent (nvidia-smi) : fonctionnement sur CPU",
            );
        }
        if cfg.engine.gpu == GpuMode::Nvidia {
            return Err(
                Fail::Usage("engine.gpu = nvidia mais aucun GPU NVIDIA utilisable".into()).into(),
            );
        }
        return Ok(sys);
    }
    let runtimes = docker.runtimes().await;
    if !sys.nvidia_toolkit {
        if output::confirm(
            "GPU NVIDIA détecté. Installer le NVIDIA Container Toolkit ?",
            true,
        )? {
            provision::install_nvidia_toolkit(&sys.os).await?;
        } else {
            output::warn("toolkit non installé : fonctionnement sur CPU");
            return Ok(sys);
        }
    } else if !runtimes.iter().any(|r| r == "nvidia") {
        provision::configure_nvidia_runtime().await?;
    }
    Docker::connect()
        .await
        .context("Docker après configuration du GPU")?;
    let mut sys = sys;
    sys.nvidia_toolkit = which("nvidia-ctk") || which("nvidia-container-runtime");
    Ok(sys)
}

fn apply_flags(cfg: &mut Config, a: &InstallArgs) -> anyhow::Result<()> {
    if let Some(e) = a.engine {
        cfg.engine.kind = match e {
            EngineArg::Ollama => EngineKind::Ollama,
            EngineArg::Vllm => EngineKind::Vllm,
        };
    }
    if let Some(g) = a.gpu {
        cfg.engine.gpu = match g {
            GpuArg::Auto => GpuMode::Auto,
            GpuArg::Nvidia => GpuMode::Nvidia,
            GpuArg::Cpu => GpuMode::Cpu,
        };
    }
    if a.webui {
        cfg.webui.enabled = true;
    }
    if let Some(p) = a.port {
        cfg.set("server.port", &p.to_string())?;
    }
    if let Some(l) = &a.listen {
        cfg.set("server.listen", l)?;
        if !is_loopback(l) {
            output::warn(format!(
                "l'API écoutera sur {l} : elle sera joignable depuis le réseau (clé API obligatoire)"
            ));
        }
    }
    if let Some(d) = &a.domain {
        cfg.set("server.domain", d)?;
    }
    if let Some(e) = &a.email {
        cfg.set("server.email", e)?;
    }
    cfg.validate()
}

fn choose_models(
    cfg: &Config,
    a: &InstallArgs,
    reco: &Recommendation,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    if a.no_model {
        return Ok((None, None));
    }
    let chat = if let Some(m) = &a.model {
        m.clone()
    } else if !cfg.engine.default_model.is_empty() {
        cfg.engine.default_model.clone()
    } else if cfg.engine.kind == EngineKind::Vllm {
        return Err(Fail::Usage(
            "avec --engine vllm, précisez le modèle Hugging Face : --model <organisation/modèle>"
                .into(),
        )
        .into());
    } else {
        pick_chat(reco)?
    };
    let embed = if a.no_embeddings {
        None
    } else if let Some(m) = &a.embedding_model {
        Some(m.clone())
    } else if !cfg.engine.embedding_model.is_empty() {
        Some(cfg.engine.embedding_model.clone())
    } else {
        Some(reco.embedding.name.to_string())
    };
    Ok((Some(chat), embed))
}

fn pick_chat(reco: &Recommendation) -> anyhow::Result<String> {
    let label = |e: &catalog::Entry| {
        format!(
            "{:<22} {:>5.1} Go  {}",
            e.name, e.download_gb, e.description
        )
    };
    output::line(format!(
        "\nMémoire utilisable pour les modèles : {:.1} Go ({})",
        reco.budget_gb,
        if reco.basis == "gpu" { "VRAM" } else { "RAM" }
    ));
    let interactive = !output::assume_yes()
        && std::io::IsTerminal::is_terminal(&std::io::stdin())
        && !output::json_mode();
    if !interactive {
        output::line(format!("Modèle recommandé : {}", bold(reco.chat.name)));
        return Ok(reco.chat.name.to_string());
    }
    let mut choices: Vec<&catalog::Entry> = vec![reco.chat];
    choices.extend(
        reco.alternatives
            .iter()
            .copied()
            .filter(|e| e.name != reco.chat.name),
    );
    let items: Vec<String> = choices
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if i == 0 {
                format!("{} (recommandé)", label(e))
            } else {
                label(e)
            }
        })
        .collect();
    let idx = dialoguer::Select::new()
        .with_prompt("Modèle de chat à installer")
        .items(&items)
        .default(0)
        .interact()?;
    Ok(choices[idx].name.to_string())
}

fn summary(
    cfg: &Config,
    sys: &SystemInfo,
    gpu: bool,
    key: Option<&CreatedKey>,
    existing_keys: usize,
) -> anyhow::Result<()> {
    let url = cfg.public_url();
    if output::json_mode() {
        return output::print_json(&json!({
            "ok": true,
            "url": url,
            "webui_url": cfg.webui_url(),
            "gpu": gpu,
            "default_model": cfg.engine.default_model,
            "embedding_model": cfg.engine.embedding_model,
            "api_key": key.map(|k| &k.key),
            "api_key_id": key.map(|k| &k.id),
            "ram_total": sys.ram_total,
        }));
    }
    output::line("");
    output::success(bold(&format!("{APP} est installé et démarré")));
    kv("API", &url);
    if let Some(w) = cfg.webui_url() {
        kv("Interface web", w);
    }
    kv(
        "Modèle par défaut",
        if cfg.engine.default_model.is_empty() {
            "(aucun)"
        } else {
            &cfg.engine.default_model
        },
    );
    if !cfg.engine.embedding_model.is_empty() {
        kv("Embeddings", &cfg.engine.embedding_model);
    }
    match key {
        Some(k) => {
            output::line("");
            output::line(bold("Clé API (affichée une seule fois, conservez-la) :"));
            output::line(format!("  {}", output::green(&k.key)));
            output::line("");
            output::line("Essai :");
            output::line(format!(
                "  curl {url}/v1/chat/completions \\\n    -H \"Authorization: Bearer {}\" \\\n    -H \"Content-Type: application/json\" \\\n    -d '{{\"model\":\"default\",\"messages\":[{{\"role\":\"user\",\"content\":\"Bonjour\"}}]}}'",
                k.key
            ));
        }
        None => kv(
            "Clés API",
            format!("{existing_keys} clé(s) existante(s) conservée(s)"),
        ),
    }
    output::line("");
    output::line(format!(
        "Suite : « {APP} status », « {APP} chat », « {APP} key create <nom> », « {APP} help »"
    ));
    if !cfg.is_exposed() {
        output::line(output::dim(&format!(
            "L'API n'écoute que sur cette machine. Pour l'ouvrir : « {APP} expose --domain <domaine> » (HTTPS) \
             ou « {APP} config set listen 0.0.0.0 »."
        )));
    }
    Ok(())
}
