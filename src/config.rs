//! Configuration source : `<config_dir>/<app>.toml`.
//!
//! Tous les autres fichiers (Compose, LiteLLM, Caddy) sont générés à partir
//! de cette configuration, des secrets et du matériel détecté.

use std::net::IpAddr;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::error::usage;
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct Config {
    pub server: Server,
    pub engine: Engine,
    pub webui: WebUi,
    pub limits: Limits,
    pub images: Images,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// Adresse d'écoute de l'API (127.0.0.1 tant que l'utilisateur n'ouvre pas).
    pub listen: String,
    /// Port HTTP de l'API quand aucun domaine n'est configuré.
    pub port: u16,
    /// Port d'administration de la passerelle, toujours lié à 127.0.0.1.
    pub admin_port: u16,
    /// Domaine public : active HTTPS (Let's Encrypt) sur 80/443.
    pub domain: String,
    /// Adresse e-mail transmise à Let's Encrypt (facultative).
    pub email: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Ollama,
    Vllm,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GpuMode {
    Auto,
    Nvidia,
    Cpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Engine {
    pub kind: EngineKind,
    pub gpu: GpuMode,
    /// Modèle servi sous l'alias `default`.
    pub default_model: String,
    /// Modèle d'embeddings (Ollama).
    pub embedding_model: String,
    /// Durée de maintien en mémoire d'un modèle inactif (Ollama).
    pub keep_alive: String,
    /// Requêtes traitées en parallèle par modèle (Ollama).
    pub num_parallel: u32,
    /// Modèle Hugging Face servi par vLLM.
    pub vllm_model: String,
    pub vllm_max_model_len: u32,
    pub vllm_gpu_memory_utilization: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WebUi {
    pub enabled: bool,
    /// Port local de l'interface web (lié à `server.listen`).
    pub port: u16,
    /// Domaine dédié à l'interface web (HTTPS) ; vide = accès local seulement.
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Requêtes par minute par clé (0 = illimité).
    pub rpm: u32,
    /// Tokens par minute par clé (0 = illimité).
    pub tpm: u32,
    /// Délai maximal d'une requête, en secondes.
    pub timeout: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Images {
    pub ollama: String,
    pub vllm: String,
    pub litellm: String,
    pub postgres: String,
    pub caddy: String,
    pub webui: String,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            listen: "127.0.0.1".into(),
            port: 8080,
            admin_port: 8081,
            domain: String::new(),
            email: String::new(),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            kind: EngineKind::Ollama,
            gpu: GpuMode::Auto,
            default_model: String::new(),
            embedding_model: String::new(),
            keep_alive: "10m".into(),
            num_parallel: 2,
            vllm_model: String::new(),
            vllm_max_model_len: 8192,
            vllm_gpu_memory_utilization: 0.90,
        }
    }
}

impl Default for WebUi {
    fn default() -> Self {
        WebUi {
            enabled: false,
            port: 3000,
            domain: String::new(),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            rpm: 60,
            tpm: 0,
            timeout: 600,
        }
    }
}

impl Default for Images {
    fn default() -> Self {
        Images {
            ollama: "ollama/ollama:0.40.10".into(),
            vllm: "vllm/vllm-openai:v0.32.0".into(),
            litellm: "ghcr.io/berriai/litellm-database:main-v1.83.14-stable".into(),
            postgres: "postgres:16.15-alpine".into(),
            caddy: "caddy:2.11.7-alpine".into(),
            webui: "ghcr.io/open-webui/open-webui:v0.11.4".into(),
        }
    }
}

/// Raccourcis acceptés par `config set`.
const ALIASES: &[(&str, &str)] = &[
    ("port", "server.port"),
    ("listen", "server.listen"),
    ("domain", "server.domain"),
    ("email", "server.email"),
    ("admin_port", "server.admin_port"),
    ("webui", "webui.enabled"),
    ("rpm", "limits.rpm"),
    ("tpm", "limits.tpm"),
    ("timeout", "limits.timeout"),
    ("engine", "engine.kind"),
    ("gpu", "engine.gpu"),
];

pub fn resolve_alias(key: &str) -> &str {
    ALIASES
        .iter()
        .find(|(a, _)| *a == key)
        .map(|(_, full)| *full)
        .unwrap_or(key)
}

impl Config {
    pub fn load(paths: &Paths) -> anyhow::Result<Self> {
        paths.require_installed()?;
        Self::load_file(&paths.config_file())
    }

    pub fn load_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("configuration invalide : {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        self.validate()?;
        let body = format!(
            "# Configuration {app}. Modifiable avec « {app} config set <clé> <valeur> ».\n\n{}",
            toml::to_string_pretty(self)?,
            app = crate::APP
        );
        write_atomic(&paths.config_file(), body.as_bytes(), 0o640)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.server.listen.parse::<IpAddr>().map_err(|_| {
            usage(format!(
                "server.listen : adresse IP invalide « {} »",
                self.server.listen
            ))
        })?;
        let ports = [
            ("server.port", self.server.port),
            ("server.admin_port", self.server.admin_port),
            ("webui.port", self.webui.port),
        ];
        for (name, p) in ports {
            if p == 0 {
                return Err(usage(format!(
                    "{name} : le port doit être compris entre 1 et 65535"
                )));
            }
        }
        if self.server.port == self.server.admin_port
            || (self.webui.enabled
                && (self.webui.port == self.server.port
                    || self.webui.port == self.server.admin_port))
        {
            return Err(usage(
                "les ports server.port, server.admin_port et webui.port doivent être différents",
            ));
        }
        for (name, d) in [
            ("server.domain", &self.server.domain),
            ("webui.domain", &self.webui.domain),
        ] {
            if !d.is_empty() && !valid_domain(d) {
                return Err(usage(format!("{name} : nom de domaine invalide « {d} »")));
            }
        }
        if !self.webui.domain.is_empty() && self.webui.domain == self.server.domain {
            return Err(usage("webui.domain doit être différent de server.domain"));
        }
        if !self.server.email.is_empty() && !self.server.email.contains('@') {
            return Err(usage("server.email : adresse e-mail invalide"));
        }
        if !(0.1..=1.0).contains(&self.engine.vllm_gpu_memory_utilization) {
            return Err(usage(
                "engine.vllm_gpu_memory_utilization doit être entre 0.1 et 1.0",
            ));
        }
        if self.engine.num_parallel == 0 {
            return Err(usage("engine.num_parallel doit être au moins 1"));
        }
        if self.limits.timeout == 0 {
            return Err(usage("limits.timeout doit être au moins 1 seconde"));
        }
        Ok(())
    }

    /// Valeur d'une clé (`section.champ`) sous forme de texte.
    pub fn get(&self, key: &str) -> anyhow::Result<String> {
        let key = resolve_alias(key);
        let value = toml::Value::try_from(self)?;
        let (section, field) = split_key(key)?;
        let v = value
            .get(section)
            .and_then(|s| s.get(field))
            .ok_or_else(|| unknown_key(key))?;
        Ok(match v {
            toml::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Modifie une clé en respectant le type de la valeur existante,
    /// puis valide l'ensemble. La configuration n'est pas modifiée en cas d'erreur.
    pub fn set(&mut self, key: &str, raw: &str) -> anyhow::Result<()> {
        let key = resolve_alias(key);
        let (section, field) = split_key(key)?;
        let mut value = toml::Value::try_from(&*self)?;
        let slot = value
            .get_mut(section)
            .and_then(|s| s.get_mut(field))
            .ok_or_else(|| unknown_key(key))?;
        *slot = match slot {
            toml::Value::Integer(_) => toml::Value::Integer(
                raw.parse::<i64>()
                    .map_err(|_| usage(format!("{key} : nombre entier attendu, reçu « {raw} »")))?,
            ),
            toml::Value::Float(_) => toml::Value::Float(
                raw.parse::<f64>()
                    .map_err(|_| usage(format!("{key} : nombre attendu, reçu « {raw} »")))?,
            ),
            toml::Value::Boolean(_) => toml::Value::Boolean(parse_bool(raw).ok_or_else(|| {
                usage(format!(
                    "{key} : valeur booléenne attendue (true/false, on/off), reçu « {raw} »"
                ))
            })?),
            _ => toml::Value::String(raw.trim().to_string()),
        };
        let updated: Config = value.try_into().map_err(|e: toml::de::Error| {
            usage(format!(
                "{key} : valeur invalide « {raw} » ({})",
                e.message()
            ))
        })?;
        updated.validate()?;
        *self = updated;
        Ok(())
    }

    /// Liste de toutes les clés modifiables.
    pub fn keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(toml::Value::Table(t)) = toml::Value::try_from(self) {
            for (section, fields) in t {
                if let toml::Value::Table(f) = fields {
                    for k in f.keys() {
                        out.push(format!("{section}.{k}"));
                    }
                }
            }
        }
        out
    }

    /// URL publique de l'API.
    pub fn public_url(&self) -> String {
        if !self.server.domain.is_empty() {
            return format!("https://{}", self.server.domain);
        }
        format!(
            "http://{}:{}",
            display_host(&self.server.listen),
            self.server.port
        )
    }

    /// URL utilisée localement par le CLI pour joindre l'API publique.
    pub fn local_url(&self) -> String {
        let host = match self.server.listen.as_str() {
            "0.0.0.0" => "127.0.0.1".to_string(),
            "::" => "[::1]".to_string(),
            other => display_host(other),
        };
        format!("http://{host}:{}", self.server.port)
    }

    /// URL d'administration de la passerelle (boucle locale uniquement).
    pub fn admin_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.server.admin_port)
    }

    pub fn webui_url(&self) -> Option<String> {
        if !self.webui.enabled {
            return None;
        }
        if !self.webui.domain.is_empty() {
            return Some(format!("https://{}", self.webui.domain));
        }
        Some(format!(
            "http://{}:{}",
            display_host(&self.server.listen),
            self.webui.port
        ))
    }

    pub fn is_exposed(&self) -> bool {
        !self.server.domain.is_empty() || !is_loopback(&self.server.listen)
    }
}

pub fn is_loopback(addr: &str) -> bool {
    addr.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn display_host(addr: &str) -> String {
    if addr.contains(':') {
        format!("[{addr}]")
    } else {
        addr.to_string()
    }
}

fn split_key(key: &str) -> anyhow::Result<(&str, &str)> {
    key.split_once('.').ok_or_else(|| unknown_key(key))
}

fn unknown_key(key: &str) -> anyhow::Error {
    usage(format!(
        "clé de configuration inconnue « {key} » — voir « {} config show » pour la liste",
        crate::APP
    ))
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "oui" | "1" => Some(true),
        "false" | "off" | "no" | "non" | "0" => Some(false),
        _ => None,
    }
}

pub fn valid_domain(d: &str) -> bool {
    d.len() <= 253
        && d.contains('.')
        && d.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Écrit un fichier de façon atomique avec les permissions données.
pub fn write_atomic(path: &Path, content: &[u8], mode: u32) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path.parent().context("chemin sans parent")?;
    std::fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fichier")
    ));
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&tmp)
            .with_context(|| format!("écriture de {}", tmp.display()))?;
        f.write_all(content)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("écriture de {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(c.server.listen, "127.0.0.1");
        assert!(!c.is_exposed());
        assert!(!c.webui.enabled);
    }

    #[test]
    fn roundtrip_toml() {
        let c = Config::default();
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let c: Config = toml::from_str("[server]\nport = 9000\n").unwrap();
        assert_eq!(c.server.port, 9000);
        assert_eq!(c.server.listen, "127.0.0.1");
    }

    #[test]
    fn set_typed_values() {
        let mut c = Config::default();
        c.set("server.port", "9090").unwrap();
        assert_eq!(c.server.port, 9090);
        c.set("webui", "on").unwrap();
        assert!(c.webui.enabled);
        c.set("listen", "0.0.0.0").unwrap();
        assert!(c.is_exposed());
        c.set("engine.kind", "vllm").unwrap();
        assert_eq!(c.engine.kind, EngineKind::Vllm);
    }

    #[test]
    fn set_rejects_bad_values_without_change() {
        let mut c = Config::default();
        let before = c.clone();
        for (k, v) in [
            ("server.port", "abc"),
            ("server.port", "70000"),
            ("server.listen", "pas-une-ip"),
            ("server.domain", "bad_domain"),
            ("engine.kind", "tgi"),
            ("webui.enabled", "peut-être"),
            ("nope.nope", "1"),
            ("sans_point", "1"),
            ("server.admin_port", "8080"),
        ] {
            let err = c.set(k, v).unwrap_err();
            assert_eq!(crate::error::exit_code(&err), 2, "{k}={v}");
            assert_eq!(c, before);
        }
    }

    #[test]
    fn get_values() {
        let c = Config::default();
        assert_eq!(c.get("port").unwrap(), "8080");
        assert_eq!(c.get("server.listen").unwrap(), "127.0.0.1");
        assert!(c.get("x.y").is_err());
    }

    #[test]
    fn urls() {
        let mut c = Config::default();
        assert_eq!(c.public_url(), "http://127.0.0.1:8080");
        c.set("listen", "0.0.0.0").unwrap();
        assert_eq!(c.local_url(), "http://127.0.0.1:8080");
        c.set("domain", "api.exemple.fr").unwrap();
        assert_eq!(c.public_url(), "https://api.exemple.fr");
        assert_eq!(c.admin_url(), "http://127.0.0.1:8081");
    }

    #[test]
    fn domains() {
        assert!(valid_domain("api.exemple.fr"));
        assert!(!valid_domain("localhost"));
        assert!(!valid_domain("-a.fr"));
        assert!(!valid_domain("a b.fr"));
    }
}
