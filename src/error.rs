//! Erreurs typées et codes de retour du CLI.

use crate::APP;

/// Erreurs qui portent un code de retour spécifique.
/// Toute autre erreur (anyhow) sort avec le code 1.
#[derive(thiserror::Error, Debug)]
pub enum Fail {
    #[error("{0}")]
    Usage(String),
    #[error(
        "{APP} n'est pas installé (aucune configuration dans {0}). Lancez « sudo {APP} install »."
    )]
    NotInstalled(String),
    #[error("{0}")]
    Docker(String),
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    Unhealthy(String),
    #[error("{0}")]
    NotFound(String),
    #[error("Opération annulée.")]
    Cancelled,
    #[error("{0}")]
    Problems(String),
}

impl Fail {
    pub fn code(&self) -> i32 {
        match self {
            Fail::Usage(_) => 2,
            Fail::NotInstalled(_) => 3,
            Fail::Docker(_) => 4,
            Fail::Permission(_) => 5,
            Fail::Unhealthy(_) => 6,
            Fail::NotFound(_) => 7,
            Fail::Cancelled => 8,
            Fail::Problems(_) => 10,
        }
    }
}

/// Code de retour associé à une erreur quelconque.
pub fn exit_code(err: &anyhow::Error) -> i32 {
    err.chain()
        .find_map(|e| e.downcast_ref::<Fail>())
        .map(Fail::code)
        .unwrap_or(1)
}

pub fn usage(msg: impl Into<String>) -> anyhow::Error {
    Fail::Usage(msg.into()).into()
}

pub fn not_found(msg: impl Into<String>) -> anyhow::Error {
    Fail::NotFound(msg.into()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn code_survives_context() {
        let err: anyhow::Result<()> = Err(Fail::NotFound("x".into()).into());
        let err = err.context("contexte").unwrap_err();
        assert_eq!(exit_code(&err), 7);
    }

    #[test]
    fn generic_error_is_one() {
        assert_eq!(exit_code(&anyhow::anyhow!("boom")), 1);
    }
}
