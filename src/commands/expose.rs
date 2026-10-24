use std::time::{Duration, Instant};

use serde_json::json;

use super::Ctx;
use crate::config::Config;
use crate::docker::Docker;
use crate::gateway::http_client;
use crate::output::{self, dim};
use crate::system::{port_free, which};
use crate::{APP, render, stack};

pub async fn run(
    ctx: &Ctx,
    domain: Option<String>,
    email: Option<String>,
    off: bool,
) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    let docker = Docker::connect().await?;

    if off {
        cfg.set("server.domain", "")?;
        cfg.save(&ctx.paths)?;
        let changes = render::regenerate(&cfg, &ctx.paths)?;
        stack::apply(&ctx.paths, &cfg, &changes).await?;
        if output::json_mode() {
            return output::print_json(&json!({ "ok": true, "url": cfg.public_url() }));
        }
        output::success(format!("HTTPS désactivé — API sur {}", cfg.public_url()));
        return Ok(());
    }

    let domain = domain.unwrap_or_default().trim().to_lowercase();
    cfg.set("server.domain", &domain)?;
    if let Some(e) = &email {
        cfg.set("server.email", e)?;
    }

    // Le domaine doit pointer vers cette machine.
    match tokio::net::lookup_host((domain.as_str(), 443)).await {
        Ok(addrs) => {
            let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
            output::kv("DNS", format!("{domain} → {}", ips.join(", ")));
            if ips.iter().any(|ip| ip.starts_with("127.") || ip == "::1") {
                output::warn(
                    "le domaine pointe vers la boucle locale : Let's Encrypt ne pourra pas le valider",
                );
            }
        }
        Err(_) => output::warn(format!(
            "{domain} ne se résout pas encore : créez un enregistrement DNS A/AAAA vers l'IP publique de cette machine"
        )),
    }

    // Ports 80/443 : libres, ou déjà tenus par notre proxy.
    let caddy_holds = docker
        .container("caddy")
        .await?
        .map(|c| c.ports.iter().any(|p| p.ends_with("->443")))
        .unwrap_or(false);
    if !caddy_holds {
        for port in [80u16, 443] {
            if !port_free("0.0.0.0", port) {
                return Err(crate::error::usage(format!(
                    "le port {port} est déjà utilisé par un autre programme (serveur web ?) : libérez-le puis relancez"
                )));
            }
        }
    }
    firewall_hint();

    cfg.save(&ctx.paths)?;
    let changes = render::regenerate(&cfg, &ctx.paths)?;
    stack::apply(&ctx.paths, &cfg, &changes).await?;

    let url = format!("https://{domain}/health");
    let bar = stack::spinner("obtention du certificat et vérification HTTPS…");
    let ok = wait_https(&url, Duration::from_secs(120)).await;
    bar.finish_and_clear();
    if output::json_mode() {
        return output::print_json(
            &json!({ "ok": true, "url": cfg.public_url(), "https_reachable": ok }),
        );
    }
    if ok {
        output::success(format!("API disponible en HTTPS : {}", cfg.public_url()));
    } else {
        output::warn(format!(
            "HTTPS configuré mais {url} ne répond pas encore. Vérifiez le DNS et l'ouverture des ports 80/443, \
             puis « {APP} logs caddy »."
        ));
    }
    Ok(())
}

async fn wait_https(url: &str, timeout: Duration) -> bool {
    let http = http_client(Duration::from_secs(10));
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(r) = http.get(url).send().await
            && r.status().is_success()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    false
}

/// Indique comment ouvrir les ports si un pare-feu local est actif.
pub fn firewall_hint() {
    let active = |cmd: &str, args: &[&str], needle: &str| {
        which(cmd)
            && std::process::Command::new(cmd)
                .args(args)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(needle))
                .unwrap_or(false)
    };
    if active("ufw", &["status"], "Status: active") {
        output::line(dim(
            "Pare-feu ufw actif : « sudo ufw allow 80,443/tcp » si ce n'est pas déjà fait.",
        ));
    } else if active("firewall-cmd", &["--state"], "running") {
        output::line(dim(
            "firewalld actif : « sudo firewall-cmd --permanent --add-service=http --add-service=https && sudo firewall-cmd --reload ».",
        ));
    }
}
