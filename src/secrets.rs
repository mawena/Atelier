//! Secrets de la pile : `<config_dir>/secrets.env` (mode 0600).
//!
//! Générés une seule fois ; les valeurs existantes ne sont jamais remplacées.

use std::collections::BTreeMap;

use anyhow::Context;
use rand::RngExt;

use crate::config::write_atomic;
use crate::paths::Paths;

pub const MASTER_KEY: &str = "LITELLM_MASTER_KEY";
pub const POSTGRES_PASSWORD: &str = "POSTGRES_PASSWORD";
pub const WEBUI_SECRET: &str = "WEBUI_SECRET_KEY";
pub const WEBUI_API_KEY: &str = "WEBUI_API_KEY";
pub const HF_TOKEN: &str = "HF_TOKEN";

#[derive(Debug, Clone, Default)]
pub struct Secrets {
    values: BTreeMap<String, String>,
}

impl Secrets {
    pub fn load(paths: &Paths) -> anyhow::Result<Self> {
        let file = paths.secrets_file();
        let raw = std::fs::read_to_string(&file).with_context(|| {
            format!("lecture de {} (droits root nécessaires ?)", file.display())
        })?;
        Ok(Self::parse(&raw))
    }

    pub fn parse(raw: &str) -> Self {
        let values = raw
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        Secrets { values }
    }

    /// Charge les secrets existants et complète ceux qui manquent.
    pub fn ensure(paths: &Paths) -> anyhow::Result<Self> {
        let mut s = if paths.secrets_file().exists() {
            Self::load(paths)?
        } else {
            Self::default()
        };
        let before = s.values.clone();
        s.fill_missing();
        if s.values != before || !paths.secrets_file().exists() {
            s.save(paths)?;
        }
        Ok(s)
    }

    pub fn fill_missing(&mut self) {
        let defaults = [
            (MASTER_KEY, format!("sk-admin-{}", random_token(40))),
            (POSTGRES_PASSWORD, random_token(32)),
            (WEBUI_SECRET, random_token(48)),
            (WEBUI_API_KEY, format!("sk-webui-{}", random_token(40))),
            (HF_TOKEN, String::new()),
        ];
        for (k, v) in defaults {
            self.values.entry(k.to_string()).or_insert(v);
        }
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        let mut body = String::from("# Secrets générés à l'installation. Ne pas partager.\n");
        for (k, v) in &self.values {
            body.push_str(&format!("{k}={v}\n"));
        }
        write_atomic(&paths.secrets_file(), body.as_bytes(), 0o600)
    }

    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn master_key(&self) -> &str {
        self.get(MASTER_KEY)
    }
}

/// Jeton alphanumérique aléatoire (sans caractère à échapper dans une URL).
pub fn random_token(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &tempfile::TempDir) -> Paths {
        Paths {
            config_dir: dir.path().join("etc"),
            data_dir: dir.path().join("data"),
            system: false,
        }
    }

    #[test]
    fn generated_once_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let p = paths(&dir);
        let a = Secrets::ensure(&p).unwrap();
        let b = Secrets::ensure(&p).unwrap();
        assert_eq!(a.master_key(), b.master_key());
        assert!(a.master_key().starts_with("sk-admin-"));
        assert_eq!(a.get(POSTGRES_PASSWORD).len(), 32);
        let mode = std::fs::metadata(p.secrets_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn existing_values_kept() {
        let mut s = Secrets::parse("LITELLM_MASTER_KEY=sk-perso\n# commentaire\n");
        s.fill_missing();
        assert_eq!(s.master_key(), "sk-perso");
        assert!(!s.get(WEBUI_API_KEY).is_empty());
    }

    #[test]
    fn tokens_are_url_safe() {
        let t = random_token(64);
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
