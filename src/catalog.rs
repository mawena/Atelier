//! Catalogue de modèles open source éprouvés et recommandations.
//!
//! Tailles : téléchargement en quantification par défaut d'Ollama (Q4 en général).
//! Mémoire minimale : RAM (CPU) ou VRAM (GPU) conseillée avec un contexte usuel.

use serde::Serialize;

use crate::system::SystemInfo;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Chat,
    Code,
    Reasoning,
    Vision,
    Embedding,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Chat => "chat",
            Kind::Code => "code",
            Kind::Reasoning => "raisonnement",
            Kind::Vision => "chat+vision",
            Kind::Embedding => "embeddings",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: &'static str,
    pub family: &'static str,
    pub kind: Kind,
    pub download_gb: f32,
    pub min_memory_gb: f32,
    /// Retenu pour les recommandations automatiques.
    pub featured: bool,
    pub description: &'static str,
}

const fn e(
    name: &'static str,
    family: &'static str,
    kind: Kind,
    download_gb: f32,
    min_memory_gb: f32,
    featured: bool,
    description: &'static str,
) -> Entry {
    Entry {
        name,
        family,
        kind,
        download_gb,
        min_memory_gb,
        featured,
        description,
    }
}

pub const CATALOG: &[Entry] = &[
    e(
        "gemma3:1b",
        "Gemma",
        Kind::Chat,
        0.8,
        2.0,
        true,
        "Google, très léger, pour petites machines",
    ),
    e(
        "qwen3:1.7b",
        "Qwen",
        Kind::Chat,
        1.4,
        3.0,
        false,
        "Alibaba, multilingue, très léger",
    ),
    e(
        "llama3.2:3b",
        "Llama",
        Kind::Chat,
        2.0,
        4.0,
        false,
        "Meta, compact et polyvalent",
    ),
    e(
        "qwen3:4b",
        "Qwen",
        Kind::Chat,
        2.5,
        5.0,
        true,
        "Alibaba, multilingue, bon rapport qualité/taille",
    ),
    e(
        "gemma3:4b",
        "Gemma",
        Kind::Vision,
        3.3,
        6.0,
        false,
        "Google, compact, comprend les images",
    ),
    e(
        "mistral:7b",
        "Mistral",
        Kind::Chat,
        4.1,
        7.0,
        false,
        "Mistral AI, classique et rapide",
    ),
    e(
        "qwen3:8b",
        "Qwen",
        Kind::Chat,
        5.2,
        8.0,
        true,
        "Alibaba, excellent généraliste 8B, multilingue",
    ),
    e(
        "llama3.1:8b",
        "Llama",
        Kind::Chat,
        4.9,
        8.0,
        false,
        "Meta, généraliste 8B, contexte 128k",
    ),
    e(
        "deepseek-r1:8b",
        "DeepSeek",
        Kind::Reasoning,
        5.2,
        8.0,
        false,
        "DeepSeek, raisonnement pas à pas",
    ),
    e(
        "mistral-nemo:12b",
        "Mistral",
        Kind::Chat,
        7.1,
        11.0,
        false,
        "Mistral AI / NVIDIA, contexte 128k",
    ),
    e(
        "gemma3:12b",
        "Gemma",
        Kind::Vision,
        8.1,
        12.0,
        false,
        "Google, multilingue, comprend les images",
    ),
    e(
        "qwen3:14b",
        "Qwen",
        Kind::Chat,
        9.3,
        14.0,
        true,
        "Alibaba, généraliste solide 14B",
    ),
    e(
        "deepseek-r1:14b",
        "DeepSeek",
        Kind::Reasoning,
        9.0,
        14.0,
        false,
        "DeepSeek, raisonnement 14B",
    ),
    e(
        "gpt-oss:20b",
        "GPT-OSS",
        Kind::Reasoning,
        14.0,
        16.0,
        false,
        "OpenAI, poids ouverts, raisonnement",
    ),
    e(
        "mistral-small3.2:24b",
        "Mistral",
        Kind::Vision,
        15.0,
        20.0,
        false,
        "Mistral AI, très bon en français",
    ),
    e(
        "gemma3:27b",
        "Gemma",
        Kind::Vision,
        17.0,
        22.0,
        false,
        "Google, haut de gamme mono-GPU",
    ),
    e(
        "qwen3:30b",
        "Qwen",
        Kind::Chat,
        19.0,
        24.0,
        true,
        "Alibaba, MoE rapide (3B actifs)",
    ),
    e(
        "qwen3:32b",
        "Qwen",
        Kind::Chat,
        20.0,
        26.0,
        false,
        "Alibaba, dense 32B",
    ),
    e(
        "deepseek-r1:32b",
        "DeepSeek",
        Kind::Reasoning,
        20.0,
        26.0,
        false,
        "DeepSeek, raisonnement 32B",
    ),
    e(
        "llama3.3:70b",
        "Llama",
        Kind::Chat,
        43.0,
        52.0,
        true,
        "Meta, 70B, très haute qualité",
    ),
    e(
        "qwen2.5:72b",
        "Qwen",
        Kind::Chat,
        47.0,
        56.0,
        false,
        "Alibaba, 72B",
    ),
    e(
        "gpt-oss:120b",
        "GPT-OSS",
        Kind::Reasoning,
        65.0,
        72.0,
        true,
        "OpenAI, poids ouverts, très grand modèle",
    ),
    e(
        "qwen2.5-coder:1.5b",
        "Qwen",
        Kind::Code,
        1.0,
        3.0,
        false,
        "Code, autocomplétion légère",
    ),
    e(
        "qwen2.5-coder:7b",
        "Qwen",
        Kind::Code,
        4.7,
        8.0,
        false,
        "Code, 7B",
    ),
    e(
        "qwen2.5-coder:14b",
        "Qwen",
        Kind::Code,
        9.0,
        14.0,
        false,
        "Code, 14B",
    ),
    e(
        "devstral:24b",
        "Mistral",
        Kind::Code,
        14.0,
        20.0,
        false,
        "Mistral AI, agents de code",
    ),
    e(
        "qwen2.5-coder:32b",
        "Qwen",
        Kind::Code,
        20.0,
        26.0,
        false,
        "Code, 32B",
    ),
    e(
        "all-minilm",
        "MiniLM",
        Kind::Embedding,
        0.05,
        1.0,
        false,
        "Embeddings anglais, 384 dimensions, minuscule",
    ),
    e(
        "nomic-embed-text",
        "Nomic",
        Kind::Embedding,
        0.27,
        1.0,
        true,
        "Embeddings, 768 dimensions, léger",
    ),
    e(
        "mxbai-embed-large",
        "mxbai",
        Kind::Embedding,
        0.67,
        2.0,
        false,
        "Embeddings anglais, 1024 dimensions",
    ),
    e(
        "bge-m3",
        "BGE",
        Kind::Embedding,
        1.2,
        3.0,
        true,
        "Embeddings multilingues (français), 1024 dimensions",
    ),
];

pub fn find(name: &str) -> Option<&'static Entry> {
    CATALOG
        .iter()
        .find(|e| e.name == name || e.name == format!("{name}:latest"))
}

pub fn search(term: &str) -> Vec<&'static Entry> {
    let t = term.to_lowercase();
    CATALOG
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&t)
                || e.family.to_lowercase().contains(&t)
                || e.kind.label().contains(&t)
                || e.description.to_lowercase().contains(&t)
                || (t.starts_with("embed") && e.kind == Kind::Embedding)
        })
        .collect()
}

/// Indique si un nom de modèle désigne a priori un modèle d'embeddings.
pub fn looks_like_embedding(name: &str) -> bool {
    if let Some(e) = find(name) {
        return e.kind == Kind::Embedding;
    }
    let n = name.to_lowercase();
    ["embed", "bge-", "minilm", "e5-", "gte-", "arctic-embed"]
        .iter()
        .any(|k| n.contains(k))
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    /// Mémoire utilisable pour les modèles (Go).
    pub budget_gb: f32,
    /// "gpu" ou "cpu".
    pub basis: &'static str,
    pub chat: &'static Entry,
    pub embedding: &'static Entry,
    /// Autres modèles de chat compatibles, du plus grand au plus petit.
    pub alternatives: Vec<&'static Entry>,
}

/// Budget mémoire disponible pour les modèles.
pub fn budget_gb(sys: &SystemInfo, gpu: bool) -> (f32, &'static str) {
    const GB: f32 = 1024.0 * 1024.0 * 1024.0;
    if gpu && sys.vram_total() > 0 {
        return (sys.vram_total() as f32 / GB * 0.9, "gpu");
    }
    let ram = sys.ram_total as f32 / GB;
    // La pile (passerelle, base, proxy) consomme environ 1,5 Go.
    ((ram - 1.5).min(ram * 0.75).max(0.0), "cpu")
}

pub fn recommend(sys: &SystemInfo, gpu: bool) -> Recommendation {
    let (budget, basis) = budget_gb(sys, gpu);
    // Sur CPU, au-delà de ~8B la génération devient trop lente pour un usage interactif.
    let cap = if basis == "cpu" {
        budget.min(10.0)
    } else {
        budget
    };
    let chat_models: Vec<&Entry> = CATALOG
        .iter()
        .filter(|e| matches!(e.kind, Kind::Chat | Kind::Vision | Kind::Reasoning))
        .collect();
    let chat = chat_models
        .iter()
        .filter(|e| e.featured && e.min_memory_gb <= cap)
        .max_by(|a, b| a.min_memory_gb.total_cmp(&b.min_memory_gb))
        .copied()
        .unwrap_or(&CATALOG[0]);
    let mut alternatives: Vec<&Entry> = chat_models
        .iter()
        .filter(|e| e.min_memory_gb <= budget && e.name != chat.name)
        .copied()
        .collect();
    alternatives.sort_by(|a, b| b.min_memory_gb.total_cmp(&a.min_memory_gb));
    alternatives.truncate(6);
    let embedding_name = if budget >= 6.0 {
        "bge-m3"
    } else {
        "nomic-embed-text"
    };
    Recommendation {
        budget_gb: budget,
        basis,
        chat,
        embedding: find(embedding_name).expect("modèle d'embeddings du catalogue"),
        alternatives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::{Gpu, OsInfo};

    fn sys(ram_gb: u64, vram_gb: Option<u64>) -> SystemInfo {
        SystemInfo {
            os: OsInfo::default(),
            arch: "x86_64".into(),
            cpus: 8,
            ram_total: ram_gb << 30,
            ram_available: ram_gb << 30,
            disk_free: 100 << 30,
            gpus: vram_gb
                .map(|v| {
                    vec![Gpu {
                        name: "GPU".into(),
                        vram_total_mb: v * 1024,
                        vram_used_mb: 0,
                        driver: "x".into(),
                    }]
                })
                .unwrap_or_default(),
            nvidia_toolkit: vram_gb.is_some(),
        }
    }

    #[test]
    fn small_vps() {
        let r = recommend(&sys(4, None), false);
        assert_eq!(r.basis, "cpu");
        assert_eq!(r.chat.name, "gemma3:1b");
        assert_eq!(r.embedding.name, "nomic-embed-text");
    }

    #[test]
    fn medium_cpu_is_capped() {
        let r = recommend(&sys(64, None), false);
        assert_eq!(r.chat.name, "qwen3:8b");
        assert_eq!(r.embedding.name, "bge-m3");
        assert!(r.alternatives.iter().any(|e| e.min_memory_gb > 10.0));
    }

    #[test]
    fn gpu_uses_vram() {
        assert_eq!(recommend(&sys(32, Some(24)), true).chat.name, "qwen3:14b");
        assert_eq!(recommend(&sys(32, Some(48)), true).chat.name, "qwen3:30b");
        assert_eq!(
            recommend(&sys(128, Some(64)), true).chat.name,
            "llama3.3:70b"
        );
        assert_eq!(
            recommend(&sys(128, Some(80)), true).chat.name,
            "gpt-oss:120b"
        );
        // GPU présent mais non utilisé : on revient à la RAM.
        assert_eq!(recommend(&sys(8, Some(24)), false).basis, "cpu");
    }

    #[test]
    fn search_and_kinds() {
        assert!(search("qwen").len() >= 5);
        assert!(search("embed").iter().all(|e| e.kind == Kind::Embedding));
        assert!(looks_like_embedding("nomic-embed-text"));
        assert!(looks_like_embedding("bge-m3"));
        assert!(!looks_like_embedding("qwen3:8b"));
        assert!(find("all-minilm").is_some());
    }

    #[test]
    fn catalog_is_consistent() {
        let mut names: Vec<_> = CATALOG.iter().map(|e| e.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), CATALOG.len(), "noms en double");
        for e in CATALOG {
            assert!(e.min_memory_gb >= e.download_gb, "{}", e.name);
        }
    }
}
