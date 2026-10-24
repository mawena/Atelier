//! Sauvegarde et restauration : configuration, secrets, base des clés,
//! données de l'interface web et, sur demande, modèles.

use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Ctx;
use crate::config::Config;
use crate::docker::{self, Docker};
use crate::error::{Fail, usage};
use crate::output;
use crate::secrets::{POSTGRES_PASSWORD, Secrets};
use crate::{APP, VERSION, render, stack};

#[derive(Serialize, Deserialize)]
struct Manifest {
    app: String,
    version: String,
    created: String,
    with_models: bool,
    database: bool,
}

pub async fn backup(ctx: &Ctx, out: Option<PathBuf>, with_models: bool) -> anyhow::Result<()> {
    let paths = &ctx.paths;
    paths.require_privileges()?;
    Config::load(paths)?;
    let docker = Docker::connect().await?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let out = out.unwrap_or_else(|| paths.backups_dir().join(format!("{APP}-{stamp}.tar.gz")));
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Dump de la base (clés, limites, journaux).
    let dump = paths.backups_dir().join(format!(".dump-{stamp}.sql"));
    std::fs::create_dir_all(paths.backups_dir())?;
    let database = match docker.running("postgres").await {
        Ok(pg) => {
            output::step("export de la base des clés");
            let mut f = tokio::fs::File::create(&dump).await?;
            docker
                .exec_to_file(
                    &pg.id,
                    &[
                        "pg_dump",
                        "-U",
                        "litellm",
                        "-d",
                        "litellm",
                        "--clean",
                        "--if-exists",
                    ],
                    &mut f,
                )
                .await?;
            true
        }
        Err(_) => {
            output::warn(
                "base arrêtée : les clés API ne seront pas sauvegardées (« start » puis recommencez)",
            );
            false
        }
    };

    output::step(format!("création de {}", out.display()));
    let manifest = Manifest {
        app: APP.into(),
        version: VERSION.into(),
        created: chrono::Local::now().to_rfc3339(),
        with_models,
        database,
    };
    let result = (|| -> anyhow::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&out)
            .with_context(|| format!("écriture de {}", out.display()))?;
        let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::default()));
        tar.follow_symlinks(false);
        let m = serde_json::to_vec_pretty(&manifest)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(m.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        tar.append_data(&mut header, "manifest.json", m.as_slice())?;
        tar.append_dir_all("config", &paths.config_dir)?;
        if database {
            tar.append_path_with_name(&dump, "postgres.sql")?;
        }
        if paths.data("webui").exists() {
            tar.append_dir_all("webui", paths.data("webui"))?;
        }
        if with_models && paths.data("ollama").exists() {
            tar.append_dir_all("ollama", paths.data("ollama"))?;
        }
        tar.into_inner()?.finish()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&dump);
    result?;

    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    if output::json_mode() {
        return output::print_json(
            &json!({ "ok": true, "file": out, "size": size, "database": database, "with_models": with_models }),
        );
    }
    output::success(format!(
        "sauvegarde créée : {} ({})",
        out.display(),
        output::bytes(size)
    ));
    output::line(output::dim(
        "Elle contient les secrets : conservez-la en lieu sûr.",
    ));
    Ok(())
}

pub async fn restore(ctx: &Ctx, file: &Path) -> anyhow::Result<()> {
    let paths = &ctx.paths;
    paths.require_privileges()?;
    if !file.is_file() {
        return Err(crate::error::not_found(format!(
            "archive introuvable : {}",
            file.display()
        )));
    }
    let manifest = read_manifest(file)?;
    output::kv(
        "Archive",
        format!(
            "{} {} du {}",
            manifest.app, manifest.version, manifest.created
        ),
    );
    output::warn("la configuration, les secrets et les clés API actuels seront remplacés");
    if !output::confirm("Restaurer cette sauvegarde ?", false)? {
        return Err(Fail::Cancelled.into());
    }
    let docker = Docker::connect().await?;
    if paths.compose_file().exists() {
        output::step("arrêt de la pile");
        docker::compose(paths, &["down", "--remove-orphans"]).await?;
    }

    output::step("extraction");
    let staging = paths.data_dir.join(".restore");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    extract(file, &staging)?;
    replace_dir(&staging.join("config"), &paths.config_dir)?;
    if staging.join("webui").exists() {
        replace_dir(&staging.join("webui"), &paths.data("webui"))?;
    }
    if staging.join("ollama").exists() {
        replace_dir(&staging.join("ollama"), &paths.data("ollama"))?;
    }

    let cfg = Config::load(paths)?;
    let secrets = Secrets::ensure(paths)?;
    render::regenerate(&cfg, paths)?;

    if staging.join("postgres.sql").exists() {
        output::step("restauration de la base des clés");
        docker::compose(paths, &["up", "-d", "postgres"]).await?;
        let pg = wait_postgres(&docker).await?;
        // Le mot de passe restauré doit correspondre à celui de la base existante.
        let pw = secrets.get(POSTGRES_PASSWORD).replace('\'', "''");
        let (code, out) = docker
            .exec(
                &pg,
                &[
                    "psql",
                    "-U",
                    "litellm",
                    "-d",
                    "litellm",
                    "-c",
                    &format!("ALTER USER litellm WITH PASSWORD '{pw}';"),
                ],
            )
            .await?;
        if code != 0 {
            anyhow::bail!("mise à jour du mot de passe de la base : {out}");
        }
        let dump = tokio::fs::File::open(staging.join("postgres.sql")).await?;
        let mut errors = String::new();
        let code = docker
            .exec_with(
                &pg,
                &["psql", "-q", "-U", "litellm", "-d", "litellm"],
                Some(dump),
                |s| {
                    if s.contains("ERROR") {
                        errors.push_str(s);
                    }
                },
            )
            .await?;
        if code != 0 || !errors.is_empty() {
            output::warn(format!(
                "import de la base terminé avec des erreurs : {}",
                errors.trim()
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&staging);

    let changes = render::regenerate(&cfg, paths)?;
    stack::apply(paths, &cfg, &changes).await?;
    if docker.running("ollama").await.is_ok()
        && let Err(e) = stack::sync_models(paths, &cfg, &docker).await
    {
        output::warn(format!("synchronisation des modèles : {e:#}"));
    }
    if output::json_mode() {
        return output::print_json(&json!({ "ok": true, "restored": file }));
    }
    output::success("sauvegarde restaurée, pile redémarrée");
    if !manifest.with_models {
        output::line(output::dim(&format!(
            "Les modèles n'étaient pas dans l'archive : « {APP} model list » puis « {APP} model pull <nom> » si besoin."
        )));
    }
    Ok(())
}

async fn wait_postgres(docker: &Docker) -> anyhow::Result<String> {
    for _ in 0..60 {
        if let Ok(Some(c)) = docker.container("postgres").await
            && c.healthy()
            && c.health.is_some()
        {
            return Ok(c.id);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(Fail::Unhealthy("la base ne démarre pas — « logs postgres »".into()).into())
}

fn read_manifest(file: &Path) -> anyhow::Result<Manifest> {
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(file)?));
    for entry in archive.entries().context("archive illisible")? {
        let mut entry = entry?;
        if entry.path()?.as_os_str() == "manifest.json" {
            let m: Manifest =
                serde_json::from_reader(&mut entry).context("manifest.json invalide")?;
            if m.app != APP {
                return Err(usage(format!("archive d'un autre outil ({})", m.app)));
            }
            return Ok(m);
        }
    }
    Err(usage(format!(
        "{} n'est pas une sauvegarde {APP} (manifest.json absent)",
        file.display()
    )))
}

/// Extrait l'archive en refusant les chemins absolus ou remontants.
fn extract(file: &Path, dest: &Path) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(file)?));
    archive.set_preserve_permissions(true);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(usage(format!(
                "chemin refusé dans l'archive : {}",
                path.display()
            )));
        }
        entry.unpack_in(dest)?;
    }
    Ok(())
}

fn replace_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        std::fs::remove_dir_all(to).with_context(|| format!("suppression de {}", to.display()))?;
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to).with_context(|| format!("restauration de {}", to.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        let archive = dir.path().join("a.tar.gz");
        {
            let f = File::create(&archive).unwrap();
            let mut tar = tar::Builder::new(GzEncoder::new(f, Compression::default()));
            let data = b"x";
            let mut h = tar::Header::new_gnu();
            h.set_size(1);
            h.set_mode(0o600);
            // Écrit le nom brut pour contourner les garde-fous du constructeur.
            h.as_gnu_mut().unwrap().name[..9].copy_from_slice(b"../evil.t");
            h.set_cksum();
            tar.append(&h, &data[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        assert!(extract(&archive, &out).is_err());
        assert!(!dir.path().join("evil.t").exists());
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let archive = dir.path().join("b.tar.gz");
        {
            let f = File::create(&archive).unwrap();
            let mut tar = tar::Builder::new(GzEncoder::new(f, Compression::default()));
            let m = serde_json::to_vec(&Manifest {
                app: APP.into(),
                version: "0.1.0".into(),
                created: "maintenant".into(),
                with_models: false,
                database: true,
            })
            .unwrap();
            let mut h = tar::Header::new_gnu();
            h.set_size(m.len() as u64);
            h.set_mode(0o600);
            h.set_cksum();
            tar.append_data(&mut h, "manifest.json", m.as_slice())
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let m = read_manifest(&archive).unwrap();
        assert!(m.database);
    }
}
