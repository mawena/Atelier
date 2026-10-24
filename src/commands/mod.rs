//! Implémentation des commandes.

mod config;
mod expose;
mod install;
mod key;
mod lifecycle;
mod model;
mod selfupdate;
mod status;
mod update;
mod version;

use crate::cli::{Cli, Command};
use crate::paths::Paths;

/// Contexte partagé par les commandes.
pub struct Ctx {
    pub paths: Paths,
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let ctx = Ctx {
        paths: Paths::from_env(),
    };
    match cli.command {
        Command::Version => version::run(),
        Command::Help { command } => crate::cli::print_help(&command),
        Command::Config(cmd) => config::run(&ctx, cmd).await,
        Command::Install(args) => install::run(&ctx, args).await,
        Command::Uninstall { purge, images } => lifecycle::uninstall(&ctx, purge, images).await,
        Command::Start => lifecycle::start(&ctx).await,
        Command::Stop => lifecycle::stop(&ctx).await,
        Command::Restart { service } => lifecycle::restart(&ctx, service).await,
        Command::Status => status::run(&ctx).await,
        Command::Logs {
            service,
            follow,
            tail,
        } => lifecycle::logs(&ctx, service, follow, tail).await,
        Command::Update {
            images_only,
            cli_only,
            to,
        } => update::run(&ctx, images_only, cli_only, to).await,
        Command::Model(cmd) => model::run(&ctx, cmd).await,
        Command::Key(cmd) => key::run(&ctx, cmd).await,
        Command::Expose { domain, email, off } => expose::run(&ctx, domain, email, off).await,
        other => Err(anyhow::anyhow!("commande pas encore disponible : {other:?}")),
    }
}
