use serde::Serialize;

use super::Ctx;
use crate::api::Api;
use crate::catalog;
use crate::config::{Config, GpuMode, is_loopback};
use crate::docker::{self, Docker};
use crate::error::Fail;
use crate::gateway::Gateway;
use crate::output::{self, green, red, yellow};
use crate::paths::is_root;
use crate::render;
use crate::secrets::Secrets;
use crate::system::{self, SystemInfo, port_free, which};
use crate::{APP, VERSION};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Serialize)]
struct Check {
    name: String,
    level: Level,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
}

#[derive(Default)]
struct Report(Vec<Check>);

impl Report {
    fn add(&mut self, name: &str, level: Level, detail: impl Into<String>, fix: Option<String>) {
        self.0.push(Check {
            name: name.into(),
            level,
            detail: detail.into(),
            fix,
        });
    }
    fn ok(&mut self, name: &str, detail: impl Into<String>) {
        self.add(name, Level::Ok, detail, None);
    }
    fn warn(&mut self, name: &str, detail: impl Into<String>, fix: impl Into<String>) {
        self.add(name, Level::Warn, detail, Some(fix.into()));
    }
    fn fail(&mut self, name: &str, detail: impl Into<String>, fix: impl Into<String>) {
        self.add(name, Level::Fail, detail, Some(fix.into()));
    }
}

const GB: u64 = 1024 * 1024 * 1024;

pub async fn run(ctx: &Ctx) -> anyhow::Result<()> {
    let mut r = Report::default();
    let paths = &ctx.paths;
    let sys = SystemInfo::detect(&paths.data_dir);

    // Système
    if !matches!(sys.arch.as_str(), "x86_64" | "aarch64") {
        r.fail("Architecture", &sys.arch, "amd64 ou arm64 requis");
    }
    r.ok(
        "Système",
        format!("{} ({}), {} cœurs", sys.os.pretty_name, sys.arch, sys.cpus),
    );
    if paths.system && !is_root() {
        r.warn(
            "Droits",
            "exécuté sans root : certaines vérifications sont limitées",
            format!("sudo {APP} doctor"),
        );
    }

    // Disque
    let free = sys.disk_free;
    let disk = format!(
        "{} libres sur {}",
        output::bytes(free),
        paths.data_dir.display()
    );
    if free < 5 * GB {
        r.fail(
            "Espace disque",
            disk,
            "libérez de l'espace (docker system prune, modèles inutilisés)",
        );
    } else if free < 15 * GB {
        r.warn(
            "Espace disque",
            disk,
            "prévoyez de la place pour les modèles",
        );
    } else {
        r.ok("Espace disque", disk);
    }

    // Docker
    let docker = if !which("docker") {
        r.fail("Docker", "non installé", format!("sudo {APP} install"));
        None
    } else {
        match Docker::connect().await {
            Ok(d) => {
                r.ok(
                    "Docker",
                    format!("démon joignable, version {}", d.server_version().await),
                );
                Some(d)
            }
            Err(e) => {
                r.fail("Docker", format!("{e:#}"), "sudo systemctl start docker");
                None
            }
        }
    };
    if which("docker") {
        if docker::compose_available() {
            r.ok("Docker Compose", "plugin v2 disponible");
        } else {
            r.fail(
                "Docker Compose",
                "plugin v2 absent",
                format!("sudo {APP} install (installe le plugin)"),
            );
        }
    }

    // Configuration
    let cfg = match Config::load(paths) {
        Ok(c) => {
            r.ok("Configuration", paths.config_file().display().to_string());
            Some(c)
        }
        Err(e) => {
            let installed = paths.is_installed();
            r.fail(
                "Configuration",
                format!("{e:#}"),
                if installed {
                    "corrigez le fichier ou « config set »".to_string()
                } else {
                    format!("sudo {APP} install")
                },
            );
            None
        }
    };

    // GPU
    let gpu_wanted = cfg.as_ref().map(|c| c.engine.gpu).unwrap_or(GpuMode::Auto);
    if sys.has_gpu() {
        let names: Vec<_> = sys
            .gpus
            .iter()
            .map(|g| format!("{} ({} Mo)", g.name, g.vram_total_mb))
            .collect();
        r.ok("GPU", names.join(", "));
        let runtime = match &docker {
            Some(d) => d.runtimes().await.iter().any(|x| x == "nvidia"),
            None => false,
        };
        if gpu_wanted == GpuMode::Cpu {
            r.ok("Accélération", "GPU ignoré (engine.gpu = cpu)");
        } else if !sys.nvidia_toolkit {
            r.warn(
                "NVIDIA Container Toolkit",
                "absent : les modèles tournent sur CPU",
                format!("sudo {APP} install"),
            );
        } else if !runtime {
            r.warn(
                "NVIDIA Container Toolkit",
                "runtime nvidia non déclaré dans Docker",
                "sudo nvidia-ctk runtime configure --runtime=docker && sudo systemctl restart docker",
            );
        } else {
            r.ok("NVIDIA Container Toolkit", "installé et configuré");
        }
    } else if system::nvidia_pci_present() {
        r.warn(
            "GPU",
            "carte NVIDIA présente mais pilote absent",
            "installez le pilote NVIDIA (ubuntu-drivers autoinstall)",
        );
    } else if gpu_wanted == GpuMode::Nvidia {
        r.fail(
            "GPU",
            "engine.gpu = nvidia mais aucun GPU détecté",
            format!("{APP} config set gpu auto"),
        );
    } else {
        r.ok("Accélération", "CPU (aucun GPU NVIDIA)");
    }

    if let Some(cfg) = &cfg {
        check_memory(&mut r, cfg, &sys);
        let containers = match &docker {
            Some(d) => d.containers().await.unwrap_or_default(),
            None => vec![],
        };
        check_ports(&mut r, cfg, !containers.is_empty());
        check_services(&mut r, cfg, &containers);
        if containers
            .iter()
            .any(|c| c.service == "litellm" && c.running())
        {
            check_gateway(&mut r, cfg, paths).await;
        }
        check_exposure(&mut r, cfg).await;
    }

    report(r)
}

fn check_memory(r: &mut Report, cfg: &Config, sys: &SystemInfo) {
    let gpu = render::gpu_enabled(cfg, sys);
    let (budget, basis) = catalog::budget_gb(sys, gpu);
    if let Some(e) = catalog::find(&cfg.engine.default_model) {
        let detail = format!(
            "{} demande ~{:.0} Go, {:.1} Go de {basis} utilisables",
            e.name, e.min_memory_gb, budget
        );
        if e.min_memory_gb > budget {
            r.warn(
                "Mémoire",
                detail,
                format!("modèle plus petit : « {APP} model recommend »"),
            );
        } else {
            r.ok("Mémoire", detail);
        }
    }
    if cfg.engine.default_model.is_empty() {
        r.warn(
            "Modèle par défaut",
            "aucun",
            format!("sudo {APP} model pull <nom> --default"),
        );
    }
}

fn check_ports(r: &mut Report, cfg: &Config, running: bool) {
    let mut ports = vec![
        (cfg.server.listen.clone(), cfg.server.port, "server.port"),
        (
            "127.0.0.1".into(),
            cfg.server.admin_port,
            "server.admin_port",
        ),
    ];
    if cfg.webui.enabled {
        ports.push((cfg.server.listen.clone(), cfg.webui.port, "webui.port"));
    }
    if !cfg.server.domain.is_empty() {
        ports.push(("0.0.0.0".into(), 80, "HTTP (Let's Encrypt)"));
        ports.push(("0.0.0.0".into(), 443, "HTTPS"));
    }
    if running {
        r.ok("Ports", "tenus par la pile");
        return;
    }
    for (addr, port, what) in ports {
        if port_free(&addr, port) {
            r.ok(&format!("Port {port}"), format!("libre ({what})"));
        } else {
            r.fail(
                &format!("Port {port}"),
                format!("occupé par un autre programme ({what})"),
                format!("libérez-le (sudo ss -ltnp | grep :{port}) ou « {APP} config set {what} <port> »"),
            );
        }
    }
}

fn check_services(r: &mut Report, cfg: &Config, containers: &[docker::Container]) {
    if containers.is_empty() {
        r.fail("Services", "pile arrêtée", format!("sudo {APP} start"));
        return;
    }
    let mut expected = vec!["caddy", "litellm", "postgres", "ollama"];
    if cfg.webui.enabled {
        expected.push("open-webui");
    }
    if cfg.engine.kind == crate::config::EngineKind::Vllm && !cfg.engine.vllm_model.is_empty() {
        expected.push("vllm");
    }
    for svc in expected {
        match containers.iter().find(|c| c.service == svc) {
            None => r.fail(
                &format!("Service {svc}"),
                "absent",
                format!("sudo {APP} start"),
            ),
            Some(c) if c.healthy() => r.ok(&format!("Service {svc}"), c.status.clone()),
            Some(c) if c.running() && c.health.as_deref() == Some("starting") => r.warn(
                &format!("Service {svc}"),
                "en démarrage",
                "patientez quelques secondes",
            ),
            Some(c) => r.fail(
                &format!("Service {svc}"),
                format!("{} {}", c.state, c.health.clone().unwrap_or_default()),
                format!("{APP} logs {svc}"),
            ),
        }
    }
}

async fn check_gateway(r: &mut Report, cfg: &Config, paths: &crate::paths::Paths) {
    let Ok(secrets) = Secrets::load(paths) else {
        r.warn(
            "Passerelle",
            "secrets illisibles",
            format!("sudo {APP} doctor"),
        );
        return;
    };
    let gw = Gateway::new(cfg, &secrets);
    if gw.ready().await {
        r.ok("Passerelle", format!("prête ({})", cfg.admin_url()));
    } else {
        r.fail(
            "Passerelle",
            "pas prête (base de données ?)",
            format!("{APP} logs litellm"),
        );
    }
    let Ok(api) = Api::local(cfg, paths) else {
        return;
    };
    match api.health().await {
        Ok((200, Some(h))) if h.status == "ok" => {
            r.ok(
                "API publique",
                format!("{}/health → ok, version {}", cfg.local_url(), h.version),
            );
            if h.version != VERSION {
                r.warn(
                    "Version",
                    format!("API {} ≠ CLI {VERSION}", h.version),
                    format!("sudo {APP} restart"),
                );
            }
        }
        Ok((code, _)) => r.fail(
            "API publique",
            format!("/health → {code}"),
            format!("{APP} logs caddy"),
        ),
        Err(e) => r.fail(
            "API publique",
            format!("{e:#}"),
            format!("{APP} logs caddy"),
        ),
    }
    match api.models_without_key().await {
        Ok(401) => r.ok("Authentification", "requêtes sans clé refusées (401)"),
        Ok(code) => r.fail(
            "Authentification",
            format!("requête sans clé → {code}"),
            format!("{APP} logs litellm"),
        ),
        Err(_) => {}
    }
}

async fn check_exposure(r: &mut Report, cfg: &Config) {
    if !is_loopback(&cfg.server.listen) && cfg.server.domain.is_empty() {
        r.warn(
            "Exposition",
            format!("API ouverte sur {} en HTTP non chiffré", cfg.server.listen),
            format!("{APP} expose --domain <domaine> (HTTPS) ou {APP} config set listen 127.0.0.1"),
        );
    } else if cfg.server.domain.is_empty() {
        r.ok("Exposition", "locale uniquement (127.0.0.1)");
    } else {
        match tokio::net::lookup_host((cfg.server.domain.as_str(), 443)).await {
            Ok(_) => r.ok("Domaine", format!("{} se résout", cfg.server.domain)),
            Err(_) => r.fail(
                "Domaine",
                format!("{} ne se résout pas", cfg.server.domain),
                "créez l'enregistrement DNS A/AAAA vers cette machine",
            ),
        }
    }
    let firewall = if which("ufw")
        && std::process::Command::new("ufw")
            .arg("status")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Status: active"))
            .unwrap_or(false)
    {
        Some("ufw")
    } else if which("firewall-cmd")
        && std::process::Command::new("firewall-cmd")
            .arg("--state")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        Some("firewalld")
    } else {
        None
    };
    match firewall {
        Some(fw) if cfg.is_exposed() => r.warn(
            "Pare-feu",
            format!("{fw} actif"),
            if cfg.server.domain.is_empty() {
                format!("autorisez le port {}/tcp", cfg.server.port)
            } else {
                "autorisez 80/tcp et 443/tcp".to_string()
            },
        ),
        Some(fw) => r.ok(
            "Pare-feu",
            format!("{fw} actif (API locale, rien à ouvrir)"),
        ),
        None => r.ok(
            "Pare-feu",
            "aucun pare-feu local détecté (vérifiez celui de l'hébergeur)",
        ),
    }
}

fn report(r: Report) -> anyhow::Result<()> {
    let fails = r.0.iter().filter(|c| c.level == Level::Fail).count();
    let warns = r.0.iter().filter(|c| c.level == Level::Warn).count();
    if output::json_mode() {
        output::print_json(
            &serde_json::json!({ "ok": fails == 0, "failures": fails, "warnings": warns, "checks": r.0 }),
        )?;
    } else {
        for c in &r.0 {
            let mark = match c.level {
                Level::Ok => green("✔"),
                Level::Warn => yellow("!"),
                Level::Fail => red("✘"),
            };
            output::line(format!("{mark} {:<26} {}", c.name, c.detail));
            if let Some(fix) = &c.fix {
                output::line(format!(
                    "  {:<26} {}",
                    "",
                    output::cyan(&format!("→ {fix}"))
                ));
            }
        }
        output::line("");
        if fails == 0 {
            output::success(format!(
                "aucun problème bloquant ({warns} avertissement(s))"
            ));
        }
    }
    if fails > 0 {
        return Err(Fail::Problems(format!(
            "{fails} problème(s) détecté(s), {warns} avertissement(s)"
        ))
        .into());
    }
    Ok(())
}
