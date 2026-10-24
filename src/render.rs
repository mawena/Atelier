//! Génération des fichiers de la pile à partir des templates.

use std::path::{Path, PathBuf};

use anyhow::Context;
use minijinja::{Environment, Value};
use serde::{Deserialize, Serialize};

use crate::config::{Config, EngineKind, GpuMode, write_atomic};
use crate::paths::Paths;
use crate::system::SystemInfo;
use crate::{APP, VERSION};

const COMPOSE: &str = include_str!("../templates/compose.yml.j2");
const LITELLM: &str = include_str!("../templates/litellm.yaml.j2");
const CADDY: &str = include_str!("../templates/Caddyfile.j2");

pub const OLLAMA_URL: &str = "http://ollama:11434";
pub const VLLM_URL: &str = "http://vllm:8000/v1";

/// Modèle servi par la passerelle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServedModel {
    pub name: String,
    pub embedding: bool,
    #[serde(default)]
    pub vllm: bool,
}

/// Modèles installés, mémorisés dans `<config_dir>/models.json`.
pub fn load_models(paths: &Paths) -> Vec<ServedModel> {
    std::fs::read_to_string(models_file(paths))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_models(paths: &Paths, models: &[ServedModel]) -> anyhow::Result<()> {
    write_atomic(
        &models_file(paths),
        serde_json::to_string_pretty(models)?.as_bytes(),
        0o640,
    )
}

fn models_file(paths: &Paths) -> PathBuf {
    paths.config_dir.join("models.json")
}

/// Utilisation effective du GPU.
pub fn gpu_enabled(cfg: &Config, sys: &SystemInfo) -> bool {
    match cfg.engine.gpu {
        GpuMode::Cpu => false,
        GpuMode::Nvidia => true,
        GpuMode::Auto => sys.has_gpu() && sys.nvidia_toolkit,
    }
}

#[derive(Serialize)]
struct LitellmEntry {
    name: String,
    target: String,
    api_base: String,
    embedding: bool,
}

/// Entrées de la passerelle : un modèle par modèle installé,
/// plus les alias `default` et `default-embedding`.
fn gateway_entries(cfg: &Config, models: &[ServedModel]) -> Vec<LitellmEntry> {
    let entry = |name: &str, m: &ServedModel| {
        let (target, api_base) = if m.vllm {
            (format!("hosted_vllm/{}", m.name), VLLM_URL)
        } else if m.embedding {
            (format!("ollama/{}", m.name), OLLAMA_URL)
        } else {
            (format!("ollama_chat/{}", m.name), OLLAMA_URL)
        };
        LitellmEntry {
            name: name.to_string(),
            target,
            api_base: api_base.to_string(),
            embedding: m.embedding,
        }
    };
    let mut out: Vec<LitellmEntry> = models.iter().map(|m| entry(&m.name, m)).collect();
    if let Some(m) = models
        .iter()
        .find(|m| m.name == cfg.engine.default_model && !m.embedding)
    {
        out.push(entry("default", m));
    }
    if let Some(m) = models
        .iter()
        .find(|m| m.name == cfg.engine.embedding_model && m.embedding)
    {
        out.push(entry("default-embedding", m));
    }
    out
}

/// Ports publiés par caddy.
pub fn caddy_ports(cfg: &Config) -> Vec<String> {
    let host = &cfg.server.listen;
    let bind = |h: &str, p: u16, inner: u16| {
        if h.contains(':') {
            format!("[{h}]:{p}:{inner}")
        } else {
            format!("{h}:{p}:{inner}")
        }
    };
    let mut ports = vec![bind(host, cfg.server.port, 8080)];
    if !cfg.server.domain.is_empty() || (cfg.webui.enabled && !cfg.webui.domain.is_empty()) {
        ports.extend([
            "80:80".to_string(),
            "443:443".to_string(),
            "443:443/udp".to_string(),
        ]);
    }
    if cfg.webui.enabled {
        ports.push(bind(host, cfg.webui.port, 3000));
    }
    ports
}

pub struct Rendered {
    pub compose: String,
    pub litellm: String,
    pub caddy: String,
}

pub fn render(
    cfg: &Config,
    paths: &Paths,
    gpu: bool,
    models: &[ServedModel],
) -> anyhow::Result<Rendered> {
    let mut env = Environment::new();
    env.set_syntax(
        minijinja::syntax::SyntaxConfig::builder()
            .trim_blocks(true)
            .lstrip_blocks(true)
            .build()?,
    );
    env.add_template("compose", COMPOSE)?;
    env.add_template("litellm", LITELLM)?;
    env.add_template("caddy", CADDY)?;

    let vllm = cfg.engine.kind == EngineKind::Vllm && !cfg.engine.vllm_model.is_empty();
    let embedding_model = models
        .iter()
        .find(|m| m.embedding && m.name == cfg.engine.embedding_model)
        .map(|_| "default-embedding")
        .unwrap_or("");
    let webui_url = cfg.webui_url().unwrap_or_default();

    let ctx = minijinja::context! {
        app => APP,
        version => VERSION,
        project => APP,
        config_dir => paths.config_dir.display().to_string(),
        data_dir => paths.data_dir.display().to_string(),
        images => Value::from(minijinja::value::Serde(&cfg.images)),
        engine => Value::from(minijinja::value::Serde(&cfg.engine)),
        admin_port => cfg.server.admin_port,
        caddy_ports => caddy_ports(cfg),
        gpu => gpu,
        vllm => vllm,
        webui => cfg.webui.enabled,
        webui_url => webui_url,
        webui_domain => &cfg.webui.domain,
        embedding_model => embedding_model,
        domain => &cfg.server.domain,
        domains => !cfg.server.domain.is_empty() || (cfg.webui.enabled && !cfg.webui.domain.is_empty()),
        email => &cfg.server.email,
        timeout => cfg.limits.timeout,
        models => Value::from(minijinja::value::Serde(gateway_entries(cfg, models))),
    };

    Ok(Rendered {
        compose: env
            .get_template("compose")?
            .render(&ctx)
            .context("rendu de compose.yml")?,
        litellm: env
            .get_template("litellm")?
            .render(&ctx)
            .context("rendu de litellm.yaml")?,
        caddy: env
            .get_template("caddy")?
            .render(&ctx)
            .context("rendu du Caddyfile")?,
    })
}

/// Services à redémarrer après écriture des fichiers.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub compose: bool,
    pub litellm: bool,
    pub caddy: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.compose || self.litellm || self.caddy
    }
}

/// Écrit les fichiers générés et crée les répertoires de données.
pub fn write(paths: &Paths, r: &Rendered) -> anyhow::Result<Changes> {
    for dir in [
        "ollama",
        "postgres",
        "webui",
        "huggingface",
        "caddy/data",
        "caddy/config",
        "backups",
    ] {
        let d = paths.data(dir);
        std::fs::create_dir_all(&d).with_context(|| format!("création de {}", d.display()))?;
    }
    Ok(Changes {
        compose: write_if_changed(&paths.compose_file(), &r.compose)?,
        litellm: write_if_changed(&paths.litellm_file(), &r.litellm)?,
        caddy: write_if_changed(&paths.caddy_file(), &r.caddy)?,
    })
}

fn write_if_changed(path: &Path, content: &str) -> anyhow::Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(false);
    }
    // Lisible par les conteneurs (caddy et litellm ne tournent pas en root partout).
    write_atomic(path, content.as_bytes(), 0o644)?;
    Ok(true)
}

/// Régénère tout à partir de la configuration enregistrée.
pub fn regenerate(cfg: &Config, paths: &Paths) -> anyhow::Result<Changes> {
    let sys = SystemInfo::detect(&paths.data_dir);
    let models = load_models(paths);
    let r = render(cfg, paths, gpu_enabled(cfg, &sys), &models)?;
    write(paths, &r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        Paths {
            config_dir: "/etc/atelier".into(),
            data_dir: "/var/lib/atelier".into(),
            system: true,
        }
    }

    fn models() -> Vec<ServedModel> {
        vec![
            ServedModel {
                name: "qwen3:4b".into(),
                embedding: false,
                vllm: false,
            },
            ServedModel {
                name: "nomic-embed-text".into(),
                embedding: true,
                vllm: false,
            },
        ]
    }

    fn cfg() -> Config {
        let mut c = Config::default();
        c.engine.default_model = "qwen3:4b".into();
        c.engine.embedding_model = "nomic-embed-text".into();
        c
    }

    fn parse(compose: &str) -> serde_json::Value {
        serde_yaml_ng::from_str(compose).expect("YAML valide")
    }

    fn published_ports(compose: &str) -> Vec<String> {
        let doc = parse(compose);
        let mut out = vec![];
        for (_, svc) in doc["services"].as_object().unwrap() {
            if let Some(ports) = svc["ports"].as_array() {
                out.extend(ports.iter().map(|p| p.as_str().unwrap().to_string()));
            }
        }
        out
    }

    #[test]
    fn default_only_binds_loopback() {
        let r = render(&cfg(), &paths(), false, &models()).unwrap();
        let ports = published_ports(&r.compose);
        assert_eq!(ports, vec!["127.0.0.1:8080:8080", "127.0.0.1:8081:4000"]);
        assert!(r.compose.contains("internal: true"));
        assert!(!r.compose.contains("driver: nvidia"));
        assert!(!r.compose.contains("open-webui:"));
        assert!(!r.compose.contains("\n  vllm:"));
    }

    #[test]
    fn gpu_variant() {
        let r = render(&cfg(), &paths(), true, &models()).unwrap();
        assert!(r.compose.contains("driver: nvidia"));
    }

    #[test]
    fn webui_and_domain_variant() {
        let mut c = cfg();
        c.set("webui", "true").unwrap();
        c.set("domain", "api.exemple.fr").unwrap();
        c.set("email", "admin@exemple.fr").unwrap();
        let r = render(&c, &paths(), false, &models()).unwrap();
        let ports = published_ports(&r.compose);
        assert!(ports.contains(&"80:80".to_string()));
        assert!(ports.contains(&"443:443".to_string()));
        assert!(ports.contains(&"127.0.0.1:3000:3000".to_string()));
        assert!(r.compose.contains("open-webui:"));
        assert!(
            r.compose
                .contains("RAG_EMBEDDING_MODEL: \"default-embedding\"")
        );
        assert!(r.caddy.contains("api.exemple.fr {"));
        assert!(r.caddy.contains("email admin@exemple.fr"));
        assert!(!r.caddy.contains("auto_https off"));
    }

    #[test]
    fn engine_and_db_never_published() {
        let mut c = cfg();
        c.set("webui", "true").unwrap();
        c.set("listen", "0.0.0.0").unwrap();
        c.set("engine.kind", "vllm").unwrap();
        c.set("engine.vllm_model", "Qwen/Qwen3-8B").unwrap();
        let r = render(&c, &paths(), true, &models()).unwrap();
        let doc = parse(&r.compose);
        for svc in ["ollama", "vllm", "postgres", "open-webui"] {
            assert!(doc["services"][svc].is_object(), "{svc} absent");
            assert!(
                doc["services"][svc].get("ports").is_none(),
                "{svc} publie un port"
            );
        }
        assert_eq!(
            doc["services"]["postgres"]["networks"],
            serde_json::json!(["db"])
        );
        assert_eq!(doc["networks"]["db"]["internal"], true);
        assert!(r.compose.contains("- \"Qwen/Qwen3-8B\""));
    }

    #[test]
    fn litellm_models_and_aliases() {
        let r = render(&cfg(), &paths(), false, &models()).unwrap();
        let doc: serde_json::Value = serde_yaml_ng::from_str(&r.litellm).unwrap();
        assert_eq!(doc["model_list"].as_array().unwrap().len(), 4);
        assert!(r.litellm.contains("model: \"ollama_chat/qwen3:4b\""));
        assert!(r.litellm.contains("model: \"ollama/nomic-embed-text\""));
        assert!(r.litellm.contains("model_name: \"default\""));
        assert!(r.litellm.contains("model_name: \"default-embedding\""));
        assert!(r.litellm.contains("mode: embedding"));
        assert!(
            r.litellm
                .contains("master_key: os.environ/LITELLM_MASTER_KEY")
        );
    }

    #[test]
    fn litellm_without_models() {
        let r = render(&Config::default(), &paths(), false, &[]).unwrap();
        let doc: serde_json::Value = serde_yaml_ng::from_str(&r.litellm).unwrap();
        assert_eq!(doc["model_list"], serde_json::json!([]));
    }

    #[test]
    fn caddy_health_contract() {
        let r = render(&cfg(), &paths(), false, &models()).unwrap();
        assert!(
            r.caddy
                .contains(&format!("{{\"status\":\"ok\",\"version\":\"{VERSION}\"}}"))
        );
        assert!(r.caddy.contains("handle /v1/*"));
        assert!(r.caddy.contains("auto_https off"));
        assert!(r.caddy.contains("flush_interval -1"));
    }

    #[test]
    fn write_reports_changes() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = Paths {
            config_dir: dir.path().join("etc"),
            data_dir: dir.path().join("data"),
            system: false,
        };
        let r = render(&cfg(), &p, false, &models()).unwrap();
        assert!(write(&p, &r).unwrap().any());
        assert!(!write(&p, &r).unwrap().any());
        assert!(p.data("ollama").is_dir());
    }

    /// Validation par Docker Compose lui-même, si disponible.
    #[test]
    fn compose_config_is_valid() {
        let ok = std::process::Command::new("docker")
            .args(["compose", "version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("docker compose absent : test ignoré");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let p = Paths {
            config_dir: dir.path().join("etc"),
            data_dir: dir.path().join("data"),
            system: false,
        };
        let mut c = cfg();
        c.set("webui", "true").unwrap();
        c.set("engine.kind", "vllm").unwrap();
        c.set("engine.vllm_model", "Qwen/Qwen3-8B").unwrap();
        let r = render(&c, &p, true, &models()).unwrap();
        write(&p, &r).unwrap();
        crate::secrets::Secrets::ensure(&p).unwrap();
        let out = std::process::Command::new("docker")
            .args(["compose", "--env-file"])
            .arg(p.secrets_file())
            .arg("-f")
            .arg(p.compose_file())
            .args(["config", "--quiet"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[cfg(test)]
mod dump {
    /// Écrit les fichiers rendus dans `$RENDER_OUT` pour inspection manuelle.
    #[test]
    #[ignore]
    fn dump_rendered() {
        let out = std::path::PathBuf::from(std::env::var("RENDER_OUT").unwrap());
        let p = crate::paths::Paths {
            config_dir: out.join("etc"),
            data_dir: out.join("data"),
            system: false,
        };
        let mut c = crate::config::Config::default();
        c.engine.default_model = "qwen3:4b".into();
        c.engine.embedding_model = "nomic-embed-text".into();
        c.set("webui", "true").unwrap();
        c.set("domain", "api.exemple.fr").unwrap();
        c.set("webui.domain", "chat.exemple.fr").unwrap();
        let models = vec![
            super::ServedModel {
                name: "qwen3:4b".into(),
                embedding: false,
                vllm: false,
            },
            super::ServedModel {
                name: "nomic-embed-text".into(),
                embedding: true,
                vllm: false,
            },
        ];
        let r = super::render(&c, &p, false, &models).unwrap();
        super::write(&p, &r).unwrap();
    }
}
