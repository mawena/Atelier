//! Définition des commandes.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

const ABOUT: &str = "Installe et gère des modèles d'IA open source (Llama, Qwen, Mistral, DeepSeek, Gemma…) \
derrière une API compatible OpenAI, le tout dans Docker.";

const AFTER_HELP: &str = "\
Exemples :
  sudo atelier install                     installe et démarre la pile
  atelier status                           état des services
  sudo atelier model pull qwen3:8b         télécharge un modèle
  sudo atelier key create mon-app          crée une clé API
  atelier chat                             discute avec le modèle par défaut

Aide détaillée d'une commande : atelier help <commande>";

#[derive(Debug, Parser)]
#[command(name = crate::APP, version, about = ABOUT, after_help = AFTER_HELP, propagate_version = true)]
pub struct Cli {
    /// Sortie JSON (pour les scripts)
    #[arg(long, global = true, display_order = 900)]
    pub json: bool,

    /// Répond oui à toutes les confirmations
    #[arg(short = 'y', long, global = true, display_order = 900)]
    pub yes: bool,

    /// Désactive les couleurs (également via NO_COLOR)
    #[arg(long, global = true, display_order = 900)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Installe Docker si besoin, génère la configuration et démarre la pile
    #[command(
        long_about = "\
Vérifie l'OS et les ressources (RAM, disque, GPU), installe Docker et Compose s'ils sont absents, \
installe le NVIDIA Container Toolkit si un GPU NVIDIA est présent, génère la configuration dans \
/etc/atelier et les données dans /var/lib/atelier, propose un modèle adapté à la machine, crée une \
première clé API et démarre la pile.

La commande est idempotente : relancée, elle conserve configuration, secrets, modèles et clés.",
        after_help = "\
Exemples :
  sudo atelier install
  sudo atelier install --model qwen3:8b --webui --yes
  sudo atelier install --engine vllm --model Qwen/Qwen3-8B"
    )]
    Install(InstallArgs),

    /// Arrête et supprime la pile (--purge : supprime aussi modèles, données et config)
    #[command(after_help = "\
Exemples :
  sudo atelier uninstall                 conserve modèles, clés et configuration
  sudo atelier uninstall --purge         supprime tout après confirmation")]
    Uninstall {
        /// Supprime aussi les modèles, les données et la configuration
        #[arg(long)]
        purge: bool,
        /// Supprime aussi les images Docker téléchargées
        #[arg(long)]
        images: bool,
    },

    /// Démarre la pile
    Start,

    /// Arrête la pile (les données sont conservées)
    Stop,

    /// Redémarre la pile ou un service
    #[command(after_help = "Exemples :\n  sudo atelier restart\n  sudo atelier restart litellm")]
    Restart {
        /// Service à redémarrer (caddy, litellm, postgres, ollama, vllm, open-webui)
        service: Option<String>,
    },

    /// État des services, URL d'accès, GPU/CPU, mémoire et modèles chargés
    Status,

    /// Affiche les journaux d'un service ou de toute la pile
    #[command(
        after_help = "Exemples :\n  atelier logs\n  atelier logs litellm -f\n  atelier logs ollama --tail 50"
    )]
    Logs {
        /// Service (caddy, litellm, postgres, ollama, vllm, open-webui)
        service: Option<String>,
        /// Suit les journaux en continu
        #[arg(short, long)]
        follow: bool,
        /// Nombre de lignes affichées au départ
        #[arg(short = 'n', long, default_value_t = 100)]
        tail: usize,
    },

    /// Met à jour les images et le CLI sans perte de données
    #[command(after_help = "\
Exemples :
  sudo atelier update                    images et CLI
  sudo atelier update --images-only
  sudo atelier update --cli-only --to 0.2.0")]
    Update {
        /// Met à jour uniquement les images Docker
        #[arg(long, conflicts_with = "cli_only")]
        images_only: bool,
        /// Met à jour uniquement le binaire du CLI
        #[arg(long)]
        cli_only: bool,
        /// Version du CLI à installer (par défaut : la dernière)
        #[arg(long = "to", value_name = "VERSION")]
        to: Option<String>,
    },

    /// Gère les modèles (list, search, pull, remove, default, recommend)
    #[command(subcommand)]
    Model(ModelCommand),

    /// Gère les clés API (create, list, revoke)
    #[command(subcommand)]
    Key(KeyCommand),

    /// Affiche ou modifie la configuration
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Active HTTPS avec un nom de domaine (Let's Encrypt)
    #[command(
        long_about = "\
Configure Caddy pour servir l'API en HTTPS sur le domaine donné, avec un certificat Let's Encrypt \
obtenu et renouvelé automatiquement. Le domaine doit pointer vers cette machine et les ports 80 et \
443 doivent être joignables depuis Internet.",
        after_help = "\
Exemples :
  sudo atelier expose --domain api.exemple.fr --email admin@exemple.fr
  sudo atelier expose --off              revient à l'accès local uniquement"
    )]
    Expose {
        /// Nom de domaine public pointant vers cette machine
        #[arg(long, required_unless_present = "off")]
        domain: Option<String>,
        /// E-mail de contact pour Let's Encrypt
        #[arg(long)]
        email: Option<String>,
        /// Désactive l'exposition HTTPS
        #[arg(long, conflicts_with = "domain")]
        off: bool,
    },

    /// Petit chat dans le terminal pour tester un modèle
    #[command(after_help = "\
Commandes pendant le chat : /reset (efface l'historique), /model <nom>, /exit

Exemples :
  atelier chat
  atelier chat --model qwen3:8b --system \"Réponds en une phrase.\"")]
    Chat {
        /// Modèle (par défaut : le modèle par défaut)
        #[arg(short, long)]
        model: Option<String>,
        /// Message système
        #[arg(short, long)]
        system: Option<String>,
    },

    /// Requête de test avec mesure de latence et de tokens/s
    #[command(after_help = "Exemples :\n  atelier test\n  atelier test --model qwen3:8b --json")]
    Test {
        /// Modèle à tester (par défaut : le modèle par défaut)
        #[arg(short, long)]
        model: Option<String>,
        /// Prompt utilisé pour la mesure
        #[arg(short, long)]
        prompt: Option<String>,
    },

    /// Sauvegarde configuration, clés et données dans une archive
    #[command(
        after_help = "Exemples :\n  sudo atelier backup\n  sudo atelier backup --output /root/atelier.tar.gz --with-models"
    )]
    Backup {
        /// Fichier de sortie (par défaut : /var/lib/atelier/backups/atelier-<date>.tar.gz)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Inclut les modèles (archive volumineuse)
        #[arg(long)]
        with_models: bool,
    },

    /// Restaure une archive créée par « atelier backup »
    #[command(
        after_help = "Exemple :\n  sudo atelier restore /var/lib/atelier/backups/atelier-20260101-120000.tar.gz"
    )]
    Restore {
        /// Archive à restaurer
        file: PathBuf,
    },

    /// Diagnostic (Docker, ports, pare-feu, GPU, espace disque) avec suggestions
    Doctor,

    /// Affiche la version
    Version,

    /// Affiche l'aide détaillée d'une commande
    #[command(after_help = "Exemples :\n  atelier help install\n  atelier help model pull")]
    Help {
        /// Commande (et sous-commande) concernée
        command: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum EngineArg {
    Ollama,
    Vllm,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum GpuArg {
    Auto,
    Nvidia,
    Cpu,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Modèle de chat à installer (par défaut : recommandé selon la machine)
    #[arg(long)]
    pub model: Option<String>,
    /// Modèle d'embeddings (par défaut : recommandé selon la machine)
    #[arg(long)]
    pub embedding_model: Option<String>,
    /// N'installe aucun modèle d'embeddings
    #[arg(long)]
    pub no_embeddings: bool,
    /// N'installe aucun modèle
    #[arg(long, conflicts_with_all = ["model", "embedding_model"])]
    pub no_model: bool,
    /// Moteur d'inférence
    #[arg(long, value_enum)]
    pub engine: Option<EngineArg>,
    /// Utilisation du GPU
    #[arg(long, value_enum)]
    pub gpu: Option<GpuArg>,
    /// Active l'interface web Open WebUI
    #[arg(long)]
    pub webui: bool,
    /// Port de l'API
    #[arg(long)]
    pub port: Option<u16>,
    /// Adresse d'écoute (127.0.0.1 par défaut ; 0.0.0.0 pour ouvrir sur le réseau)
    #[arg(long)]
    pub listen: Option<String>,
    /// Domaine public (active HTTPS)
    #[arg(long)]
    pub domain: Option<String>,
    /// E-mail pour Let's Encrypt
    #[arg(long)]
    pub email: Option<String>,
    /// N'installe pas Docker automatiquement
    #[arg(long)]
    pub skip_docker_install: bool,
    /// Nom de la première clé API
    #[arg(long, default_value = "default")]
    pub key_name: String,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// Modèles installés
    List,
    /// Cherche dans le catalogue des modèles éprouvés
    #[command(
        after_help = "Exemples :\n  atelier model search qwen\n  atelier model search code\n  atelier model search embed"
    )]
    Search {
        /// Terme recherché (nom, famille, usage)
        term: String,
    },
    /// Télécharge un modèle (nom Ollama, ou dépôt Hugging Face avec vLLM)
    #[command(after_help = "\
Exemples :
  sudo atelier model pull qwen3:8b
  sudo atelier model pull nomic-embed-text
  sudo atelier model pull mistral:7b --default")]
    Pull {
        name: String,
        /// Définit aussi ce modèle comme modèle par défaut
        #[arg(long)]
        default: bool,
    },
    /// Supprime un modèle
    Remove {
        name: String,
        /// Supprime même s'il s'agit du modèle par défaut
        #[arg(long)]
        force: bool,
    },
    /// Définit le modèle par défaut (alias « default » de l'API)
    Default { name: String },
    /// Recommande des modèles selon la RAM/VRAM disponible
    Recommend,
}

#[derive(Debug, Subcommand)]
pub enum KeyCommand {
    /// Crée une clé API (affichée une seule fois)
    #[command(
        after_help = "Exemples :\n  sudo atelier key create android\n  sudo atelier key create batch --rpm 600 --tpm 200000"
    )]
    Create {
        name: String,
        /// Requêtes par minute (par défaut : limits.rpm)
        #[arg(long)]
        rpm: Option<u32>,
        /// Tokens par minute (par défaut : limits.tpm)
        #[arg(long)]
        tpm: Option<u32>,
        /// Durée de validité (ex. 30d, 12h) ; illimitée par défaut
        #[arg(long)]
        expires: Option<String>,
    },
    /// Liste les clés
    List,
    /// Révoque une clé (identifiant court affiché par « key list », ou nom)
    Revoke { id: String },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Affiche la configuration
    Show,
    /// Modifie une valeur et applique le changement
    #[command(
        long_about = "\
Modifie une valeur de la configuration, régénère les fichiers et redémarre les services concernés.

Clés principales :
  server.listen       adresse d'écoute (127.0.0.1, 0.0.0.0…)     alias : listen
  server.port         port de l'API                              alias : port
  server.domain       domaine HTTPS (vide pour désactiver)       alias : domain
  server.email        e-mail Let's Encrypt                       alias : email
  webui.enabled       interface web (true/false)                 alias : webui
  webui.port          port de l'interface web
  webui.domain        domaine HTTPS de l'interface web
  limits.rpm          requêtes/minute par défaut des clés        alias : rpm
  limits.tpm          tokens/minute par défaut des clés          alias : tpm
  limits.timeout      délai maximal d'une requête (s)            alias : timeout
  engine.kind         ollama ou vllm                             alias : engine
  engine.gpu          auto, nvidia ou cpu                        alias : gpu
  engine.keep_alive   maintien en mémoire d'un modèle (ex. 10m)
  engine.num_parallel requêtes parallèles par modèle
  images.<service>    image Docker d'un service",
        after_help = "\
Exemples :
  sudo atelier config set port 9000
  sudo atelier config set webui true
  sudo atelier config set listen 0.0.0.0"
    )]
    Set {
        key: String,
        value: String,
        /// Enregistre sans redémarrer les services
        #[arg(long)]
        no_apply: bool,
    },
    /// Affiche une valeur
    Get { key: String },
}

/// Construit la commande avec les textes d'aide traduits.
pub fn command() -> clap::Command {
    use clap::CommandFactory;
    localize(Cli::command().help_template(HELP_TEMPLATE), true)
}

const HELP_TEMPLATE: &str =
    "{before-help}{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}";

fn localize(cmd: clap::Command, root: bool) -> clap::Command {
    let mut cmd = cmd
        .disable_help_subcommand(true)
        .subcommand_help_heading("Commandes")
        .help_template(HELP_TEMPLATE)
        .disable_help_flag(true)
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .help("Affiche l'aide")
                .action(clap::ArgAction::Help)
                .global(true)
                .display_order(999),
        );
    if root {
        cmd = cmd.disable_version_flag(true).arg(
            clap::Arg::new("version")
                .short('V')
                .long("version")
                .help("Affiche la version")
                .action(clap::ArgAction::Version),
        );
    }
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, localize_sub);
    }
    cmd
}

fn localize_sub(cmd: clap::Command) -> clap::Command {
    let mut cmd = cmd
        .subcommand_help_heading("Commandes")
        .help_template(HELP_TEMPLATE)
        .disable_help_subcommand(true);
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, localize_sub);
    }
    cmd
}

/// Affiche l'aide longue de la commande désignée par `path`.
pub fn print_help(path: &[String]) -> anyhow::Result<()> {
    let mut cmd = command();
    cmd.build();
    let mut cur = &mut cmd;
    for name in path {
        cur = cur
            .find_subcommand_mut(name)
            .ok_or_else(|| crate::error::usage(format!("commande inconnue « {name} »")))?;
    }
    cur.print_long_help()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        command().debug_assert();
    }

    #[test]
    fn help_paths() {
        assert!(print_help(&["model".into(), "pull".into()]).is_ok());
        assert!(print_help(&["inconnue".into()]).is_err());
    }
}
