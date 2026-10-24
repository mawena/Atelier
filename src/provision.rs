//! Installation des dépendances système : Docker, Compose, NVIDIA Container Toolkit.

use std::process::Stdio;

use anyhow::Context;

use crate::output;
use crate::system::{OsFamily, OsInfo, which};

/// Exécute un script shell (`sh -euc`) en affichant sa sortie.
async fn run_script(what: &str, script: &str) -> anyhow::Result<()> {
    output::step(what);
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-euc")
        .arg(script)
        .env("DEBIAN_FRONTEND", "noninteractive");
    if output::json_mode() {
        cmd.stdout(Stdio::null());
    }
    let status = cmd
        .status()
        .await
        .with_context(|| format!("{what} : lancement impossible"))?;
    if !status.success() {
        anyhow::bail!("{what} : échec (code {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Outils de base nécessaires aux scripts d'installation.
fn prerequisites(os: &OsInfo) -> &'static str {
    match os.family() {
        OsFamily::Debian => {
            "command -v curl >/dev/null && command -v gpg >/dev/null || \
             { apt-get update -qq && apt-get install -y -qq curl ca-certificates gnupg; }"
        }
        OsFamily::RedHat => "command -v curl >/dev/null || dnf install -y curl",
        OsFamily::Suse => "command -v curl >/dev/null || zypper --non-interactive install curl",
        OsFamily::Arch => "command -v curl >/dev/null || pacman -Sy --noconfirm curl",
        OsFamily::Other => "command -v curl >/dev/null",
    }
}

/// Installe Docker Engine et le plugin Compose.
pub async fn install_docker(os: &OsInfo) -> anyhow::Result<()> {
    let install = match os.family() {
        // Script officiel : Ubuntu, Debian, Raspbian, Fedora, CentOS/RHEL…
        OsFamily::Debian | OsFamily::RedHat => {
            "curl -fsSL https://get.docker.com -o /tmp/get-docker.sh && sh /tmp/get-docker.sh && rm -f /tmp/get-docker.sh"
        }
        OsFamily::Suse => "zypper --non-interactive install docker docker-compose",
        OsFamily::Arch => "pacman -Sy --noconfirm docker docker-compose",
        OsFamily::Other => {
            "curl -fsSL https://get.docker.com -o /tmp/get-docker.sh && sh /tmp/get-docker.sh && rm -f /tmp/get-docker.sh"
        }
    };
    run_script("installation des prérequis", prerequisites(os)).await?;
    run_script("installation de Docker", install).await?;
    enable_docker().await
}

/// Installe uniquement le plugin Compose (Docker déjà présent).
pub async fn install_compose(os: &OsInfo) -> anyhow::Result<()> {
    let script = match os.family() {
        OsFamily::Debian => {
            "apt-get update -qq && (apt-get install -y -qq docker-compose-plugin || apt-get install -y -qq docker-compose-v2)"
        }
        OsFamily::RedHat => "dnf install -y docker-compose-plugin",
        OsFamily::Suse => "zypper --non-interactive install docker-compose",
        OsFamily::Arch => "pacman -Sy --noconfirm docker-compose",
        OsFamily::Other => anyhow::bail!("installez le plugin Docker Compose v2 puis relancez"),
    };
    run_script("installation de Docker Compose", script).await
}

/// Active et démarre le service Docker.
pub async fn enable_docker() -> anyhow::Result<()> {
    if which("systemctl") {
        run_script(
            "démarrage du service Docker",
            "systemctl enable --now docker",
        )
        .await?;
    } else if which("service") {
        run_script("démarrage du service Docker", "service docker start").await?;
    }
    Ok(())
}

/// Installe et configure le NVIDIA Container Toolkit.
pub async fn install_nvidia_toolkit(os: &OsInfo) -> anyhow::Result<()> {
    const REPO: &str = "https://nvidia.github.io/libnvidia-container";
    let script = match os.family() {
        OsFamily::Debian => format!(
            "curl -fsSL {REPO}/gpgkey | gpg --dearmor --yes -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg && \
             curl -fsSL {REPO}/stable/deb/nvidia-container-toolkit.list | \
             sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' \
             > /etc/apt/sources.list.d/nvidia-container-toolkit.list && \
             apt-get update -qq && apt-get install -y -qq nvidia-container-toolkit"
        ),
        OsFamily::RedHat => format!(
            "curl -fsSL {REPO}/stable/rpm/nvidia-container-toolkit.repo > /etc/yum.repos.d/nvidia-container-toolkit.repo && \
             dnf install -y nvidia-container-toolkit"
        ),
        OsFamily::Suse => format!(
            "zypper --non-interactive ar {REPO}/stable/rpm/nvidia-container-toolkit.repo || true; \
             zypper --non-interactive --gpg-auto-import-keys install nvidia-container-toolkit"
        ),
        OsFamily::Arch => "pacman -Sy --noconfirm nvidia-container-toolkit".to_string(),
        OsFamily::Other => anyhow::bail!(
            "distribution non prise en charge pour le NVIDIA Container Toolkit : \
             installez-le manuellement puis relancez"
        ),
    };
    run_script("installation des prérequis", prerequisites(os)).await?;
    run_script("installation du NVIDIA Container Toolkit", &script).await?;
    configure_nvidia_runtime().await
}

/// Déclare le runtime NVIDIA auprès de Docker puis redémarre Docker.
pub async fn configure_nvidia_runtime() -> anyhow::Result<()> {
    run_script(
        "configuration du runtime NVIDIA pour Docker",
        "nvidia-ctk runtime configure --runtime=docker && \
         (systemctl restart docker 2>/dev/null || service docker restart)",
    )
    .await
}
