mod api;
mod catalog;
mod cli;
mod commands;
mod config;
mod docker;
mod engine;
mod error;
mod gateway;
mod output;
mod paths;
mod provision;
mod render;
mod secrets;
mod sse;
mod stack;
mod system;

use clap::FromArgMatches;

/// Nom de l'outil : modifier `name` dans Cargo.toml pour le renommer.
pub const APP: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT: &str = env!("ATELIER_COMMIT");
pub const TARGET: &str = env!("ATELIER_TARGET");

fn main() {
    // Sortie vers un tube fermé (ex. « | head ») : terminer sans erreur.
    // SAFETY: appel unique au démarrage, avant tout thread.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let matches = cli::command().get_matches();
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    output::init(cli.json, cli.no_color, cli.yes);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            output::error(format!("démarrage impossible : {e}"));
            std::process::exit(1);
        }
    };

    let result = runtime.block_on(commands::run(cli));
    if let Err(err) = result {
        let code = error::exit_code(&err);
        if output::json_mode() {
            let _ = output::print_json(&serde_json::json!({
                "ok": false,
                "error": format!("{err:#}"),
                "code": code,
            }));
        } else {
            output::error(format!("{err:#}"));
        }
        std::process::exit(code);
    }
}
