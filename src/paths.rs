//! Emplacements des fichiers. Par défaut `/etc/<app>` et `/var/lib/<app>`,
//! surchargeables par `ATELIER_CONFIG_DIR` et `ATELIER_DATA_DIR`.

use std::path::{Path, PathBuf};

use crate::APP;
use crate::error::Fail;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Vrai si les emplacements par défaut sont utilisés (droits root requis).
    pub system: bool,
}

impl Paths {
    pub fn from_env() -> Self {
        let cfg = std::env::var_os("ATELIER_CONFIG_DIR").map(PathBuf::from);
        let data = std::env::var_os("ATELIER_DATA_DIR").map(PathBuf::from);
        let system = cfg.is_none() && data.is_none();
        Paths {
            config_dir: cfg.unwrap_or_else(|| PathBuf::from(format!("/etc/{APP}"))),
            data_dir: data.unwrap_or_else(|| PathBuf::from(format!("/var/lib/{APP}"))),
            system,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(format!("{APP}.toml"))
    }
    pub fn secrets_file(&self) -> PathBuf {
        self.config_dir.join("secrets.env")
    }
    pub fn compose_file(&self) -> PathBuf {
        self.config_dir.join("compose.yml")
    }
    pub fn litellm_file(&self) -> PathBuf {
        self.config_dir.join("litellm.yaml")
    }
    pub fn caddy_file(&self) -> PathBuf {
        self.config_dir.join("Caddyfile")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }
    pub fn data(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }

    pub fn is_installed(&self) -> bool {
        self.config_file().exists()
    }

    pub fn require_installed(&self) -> anyhow::Result<()> {
        if self.is_installed() {
            return Ok(());
        }
        Err(Fail::NotInstalled(self.config_dir.display().to_string()).into())
    }

    /// Vérifie que l'on peut écrire la configuration et lire les secrets.
    pub fn require_privileges(&self) -> anyhow::Result<()> {
        if is_root() {
            return Ok(());
        }
        let probe = if self.config_dir.exists() {
            self.config_dir.clone()
        } else {
            nearest_existing(&self.config_dir)
        };
        let secrets_ok = !self.secrets_file().exists() || readable(&self.secrets_file());
        if writable(&probe) && secrets_ok {
            return Ok(());
        }
        Err(Fail::Permission(format!(
            "droits insuffisants sur {} — relancez avec « sudo {APP} … »",
            self.config_dir.display()
        ))
        .into())
    }
}

pub fn is_root() -> bool {
    // SAFETY: geteuid n'a pas d'effet de bord.
    unsafe { libc::geteuid() == 0 }
}

fn nearest_existing(p: &Path) -> PathBuf {
    let mut cur = p.to_path_buf();
    while !cur.exists() {
        match cur.parent() {
            Some(parent) => cur = parent.to_path_buf(),
            None => return PathBuf::from("/"),
        }
    }
    cur
}

fn access(p: &Path, mode: libc::c_int) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: chaîne C valide, access ne modifie rien.
    unsafe { libc::access(c.as_ptr(), mode) == 0 }
}

fn writable(p: &Path) -> bool {
    access(p, libc::W_OK)
}

fn readable(p: &Path) -> bool {
    access(p, libc::R_OK)
}
