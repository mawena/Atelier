//! Détection de l'OS et du matériel.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct OsInfo {
    pub id: String,
    pub id_like: String,
    pub version_id: String,
    pub pretty_name: String,
}

impl OsInfo {
    pub fn family(&self) -> OsFamily {
        let all = format!("{} {}", self.id, self.id_like);
        if all.contains("debian") || all.contains("ubuntu") {
            OsFamily::Debian
        } else if ["rhel", "fedora", "centos", "rocky", "almalinux", "amzn"]
            .iter()
            .any(|k| all.contains(k))
        {
            OsFamily::RedHat
        } else if all.contains("suse") {
            OsFamily::Suse
        } else if all.contains("arch") {
            OsFamily::Arch
        } else {
            OsFamily::Other
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OsFamily {
    Debian,
    RedHat,
    Suse,
    Arch,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub driver: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub os: OsInfo,
    pub arch: String,
    pub cpus: usize,
    pub ram_total: u64,
    pub ram_available: u64,
    pub disk_free: u64,
    pub gpus: Vec<Gpu>,
    /// NVIDIA Container Toolkit présent (nvidia-ctk ou runtime nvidia).
    pub nvidia_toolkit: bool,
}

impl SystemInfo {
    pub fn detect(data_dir: &Path) -> Self {
        let (ram_total, ram_available) = meminfo();
        SystemInfo {
            os: os_release(),
            arch: std::env::consts::ARCH.to_string(),
            cpus: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            ram_total,
            ram_available,
            disk_free: disk_free(data_dir),
            gpus: nvidia_gpus(),
            nvidia_toolkit: which("nvidia-ctk") || which("nvidia-container-runtime"),
        }
    }

    pub fn has_gpu(&self) -> bool {
        !self.gpus.is_empty()
    }

    pub fn vram_total(&self) -> u64 {
        self.gpus
            .iter()
            .map(|g| g.vram_total_mb * 1024 * 1024)
            .sum()
    }

    pub fn vram_used(&self) -> u64 {
        self.gpus.iter().map(|g| g.vram_used_mb * 1024 * 1024).sum()
    }
}

/// Lit `/etc/os-release`.
pub fn os_release() -> OsInfo {
    std::fs::read_to_string("/etc/os-release")
        .map(|s| parse_os_release(&s))
        .unwrap_or_default()
}

pub fn parse_os_release(content: &str) -> OsInfo {
    let mut info = OsInfo::default();
    for line in content.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "ID" => info.id = v,
            "ID_LIKE" => info.id_like = v,
            "VERSION_ID" => info.version_id = v,
            "PRETTY_NAME" => info.pretty_name = v,
            _ => {}
        }
    }
    info
}

/// (total, disponible) en octets, depuis `/proc/meminfo`.
pub fn meminfo() -> (u64, u64) {
    std::fs::read_to_string("/proc/meminfo")
        .map(|s| parse_meminfo(&s))
        .unwrap_or((0, 0))
}

pub fn parse_meminfo(content: &str) -> (u64, u64) {
    let get = |key: &str| {
        content
            .lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    };
    (get("MemTotal:"), get("MemAvailable:"))
}

/// Espace libre (octets) sur le système de fichiers qui contiendra `path`.
pub fn disk_free(path: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            p = "/".into();
            break;
        }
    }
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else {
        return 0;
    };
    // SAFETY: structure initialisée à zéro, chemin C valide.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 {
            return 0;
        }
        st.f_bavail as u64 * st.f_frsize as u64
    }
}

/// GPU NVIDIA via `nvidia-smi` (vide si absent ou sans pilote).
pub fn nvidia_gpus() -> Vec<Gpu> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => parse_nvidia_smi(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

pub fn parse_nvidia_smi(content: &str) -> Vec<Gpu> {
    content
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if f.len() < 4 {
                return None;
            }
            Some(Gpu {
                name: f[0].to_string(),
                vram_total_mb: f[1].parse().ok()?,
                vram_used_mb: f[2].parse().unwrap_or(0),
                driver: f[3].to_string(),
            })
        })
        .collect()
}

/// Carte NVIDIA visible sur le bus PCI, même sans pilote installé.
pub fn nvidia_pci_present() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return false;
    };
    entries.flatten().any(|e| {
        let vendor = std::fs::read_to_string(e.path().join("vendor")).unwrap_or_default();
        let class = std::fs::read_to_string(e.path().join("class")).unwrap_or_default();
        vendor.trim() == "0x10de" && (class.starts_with("0x0300") || class.starts_with("0x0302"))
    })
}

pub fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(bin).is_file()))
        .unwrap_or(false)
}

/// Vrai si l'adresse:port peut être liée (port libre).
pub fn port_free(addr: &str, port: u16) -> bool {
    let host = if addr.contains(':') {
        format!("[{addr}]")
    } else {
        addr.to_string()
    };
    std::net::TcpListener::bind(format!("{host}:{port}")).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_ubuntu() {
        let os = parse_os_release(
            "PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nID=ubuntu\nID_LIKE=debian\n",
        );
        assert_eq!(os.id, "ubuntu");
        assert_eq!(os.version_id, "24.04");
        assert_eq!(os.family(), OsFamily::Debian);
    }

    #[test]
    fn os_family_rhel() {
        let os = parse_os_release("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n");
        assert_eq!(os.family(), OsFamily::RedHat);
    }

    #[test]
    fn meminfo_parse() {
        let (t, a) = parse_meminfo(
            "MemTotal:       16000000 kB\nMemFree: 1 kB\nMemAvailable:    8000000 kB\n",
        );
        assert_eq!(t, 16_000_000 * 1024);
        assert_eq!(a, 8_000_000 * 1024);
    }

    #[test]
    fn nvidia_smi_parse() {
        let g = parse_nvidia_smi(
            "NVIDIA GeForce RTX 4090, 24564, 1024, 560.35.03\nNVIDIA A100, 81920, 0, 560.35.03\n",
        );
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].vram_total_mb, 24564);
        assert_eq!(g[1].name, "NVIDIA A100");
    }

    #[test]
    fn detect_runs() {
        let s = SystemInfo::detect(Path::new("/"));
        assert!(s.cpus >= 1);
    }
}
