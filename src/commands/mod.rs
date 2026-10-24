//! Implémentation des commandes.

mod config;
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
        other => Err(anyhow::anyhow!(
            "commande pas encore disponible : {other:?}"
        )),
    }
}
