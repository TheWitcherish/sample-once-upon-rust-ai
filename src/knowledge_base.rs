//! The Rules Agent's knowledge base: Basic Rules passages plus their Titan
//! embeddings, in one local JSON file.
//!
//! The Rust side of the Python chapter's ChromaDB collection. Shared by the
//! builder (`ch5-create-knowledge-base`) and the Rules Agent (`ch5-rules-agent`),
//! so both always cut, embed and compare text the same way.

use std::env;
use std::path::{Path, PathBuf};

use aws_sdk_bedrockruntime::Client;
use aws_sdk_bedrockruntime::primitives::Blob;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub type KbResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Verified live (us-east-1, 2026-10-08, `aws bedrock get-foundation-model`):
/// `ACTIVE`, output modality `EMBEDDING`. Override with `EMBEDDING_MODEL_ID`.
pub const DEFAULT_EMBEDDING_MODEL_ID: &str = "amazon.titan-embed-text-v2:0";
/// Titan V2 offers 256, 512 or 1024. 512 keeps the file small and search sharp.
pub const EMBEDDING_DIMENSIONS: usize = 512;

pub const PDF_FILE: &str = "DnD_BasicRules_2018.pdf";
/// Printed on almost every page; useless for search, so it is stripped.
const FOOTER: &str = "D&D Basic Rules (Version 1.0). Not for resale. Permission granted to print and photocopy this document for personal use only.";
const CHUNK_SIZE: usize = 1000; // characters per passage
const CHUNK_OVERLAP: usize = 200; // shared by two consecutive passages, so a rule cut in two stays findable
const MIN_PASSAGE: usize = 50;

pub fn embedding_model_id() -> String {
    env::var("EMBEDDING_MODEL_ID").unwrap_or_else(|_| DEFAULT_EMBEDDING_MODEL_ID.to_string())
}

fn utils_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("5_a2a_integration/utils")
}

pub fn pdf_path() -> PathBuf {
    utils_dir().join(PDF_FILE)
}

pub fn kb_path() -> PathBuf {
    utils_dir().join("rules_knowledge_base.json")
}

/// A passage cut from the PDF, before it is embedded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub text: String,
    pub page: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Passage {
    #[serde(flatten)]
    pub chunk: Chunk,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub source: String,
    pub model_id: String,
    pub dimensions: usize,
    pub passages: Vec<Passage>,
}

fn squash_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cut each page into overlapping passages: the same algorithm as the Python
/// `extract_chunks`, counted in characters (not bytes) so the cuts land in the
/// same places and never split a multi-byte character.
pub fn extract_chunks(pages: &[String]) -> Vec<Chunk> {
    let footer = squash_whitespace(FOOTER);
    let mut chunks = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let page_number = index + 1;
        let text: Vec<char> = squash_whitespace(&squash_whitespace(page).replace(&footer, " "))
            .chars()
            .collect();
        let mut start = 0;
        while start < text.len() {
            let mut end = (start + CHUNK_SIZE).min(text.len());
            if end < text.len() {
                // Cut on a space, not in the middle of a word.
                if let Some(cut) = text[start + CHUNK_SIZE / 2..end]
                    .iter()
                    .rposition(|c| *c == ' ')
                {
                    end = start + CHUNK_SIZE / 2 + cut;
                }
            }
            let passage: String = text[start..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            if passage.chars().count() > MIN_PASSAGE {
                chunks.push(Chunk {
                    id: format!("page_{page_number}_offset_{start}"),
                    text: passage,
                    page: page_number,
                });
            }
            if end == text.len() {
                break;
            }
            start = end - CHUNK_OVERLAP;
        }
    }
    chunks
}

impl KnowledgeBase {
    /// # Errors
    ///
    /// Fails when the file is missing (with the command that builds it) or is
    /// not a knowledge base.
    pub fn load(path: &Path) -> KbResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| {
            format!(
                "cannot read {}: {e}. Build it first: cargo run --bin ch5-create-knowledge-base",
                path.display()
            )
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Write a temporary file, then rename it over the old one, so an
    /// interrupted build never leaves a half-written knowledge base.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn save(&self, path: &Path) -> KbResult<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The `k` passages closest to `query`, best first. Titan returns unit
    /// vectors (`normalize: true`), so the dot product is the cosine similarity.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, &Passage)> {
        let mut scored: Vec<(f32, &Passage)> = self
            .passages
            .iter()
            .map(|p| (dot(&p.embedding, query), p))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(k);
        scored
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Titan Text Embeddings V2 on Bedrock. `Clone` is cheap (the SDK client is
/// reference-counted), so the builder hands one copy to each concurrent task.
#[derive(Debug, Clone)]
pub struct Embedder {
    client: Client,
    model_id: String,
}

impl Embedder {
    /// Honours `EMBEDDING_MODEL_ID` and the standard AWS environment
    /// (`AWS_REGION`, `AWS_PROFILE`, ...).
    pub async fn from_env() -> Self {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        Self {
            client: Client::new(&config),
            model_id: embedding_model_id(),
        }
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// # Errors
    ///
    /// Fails when Bedrock rejects the call or answers without an embedding.
    pub async fn embed(&self, text: &str) -> KbResult<Vec<f32>> {
        let body = json!({
            "inputText": text,
            "dimensions": EMBEDDING_DIMENSIONS,
            "normalize": true,
        });
        let response = self
            .client
            .invoke_model()
            .model_id(&self.model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(serde_json::to_vec(&body)?))
            .send()
            .await
            .map_err(|e| format!("Bedrock embedding call failed: {}", e.into_service_error()))?;
        let answer: Value = serde_json::from_slice(response.body().as_ref())?;
        let embedding: Vec<f32> = serde_json::from_value(
            answer
                .get("embedding")
                .cloned()
                .ok_or("Titan answered without an embedding")?,
        )?;
        Ok(embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(n: usize) -> String {
        (0..n)
            .map(|i| format!("w{i:03}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn short_pages_are_skipped_and_the_footer_is_stripped() {
        let pages = vec![
            "Too short.".to_string(),
            format!(
                "{FOOTER}\n\n  An opportunity attack happens when a hostile creature moves out of reach."
            ),
        ];
        let chunks = extract_chunks(&pages);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].page, 2);
        assert_eq!(chunks[0].id, "page_2_offset_0");
        assert!(chunks[0].text.starts_with("An opportunity attack"));
    }

    #[test]
    fn long_pages_overlap_and_cut_on_spaces() {
        // 600 four-char words + spaces = 2999 chars: three passages.
        let chunks = extract_chunks(&[words(600)]);
        assert!(chunks.len() >= 3);
        for c in &chunks {
            assert!(c.text.chars().count() <= CHUNK_SIZE);
            assert!(c.text.split(' ').all(|w| w.len() == 4), "cut inside a word");
        }
        // The last word of a passage reappears in the next one: the overlap.
        let last = chunks[0].text.rsplit(' ').next().unwrap();
        assert!(chunks[1].text.contains(last));
    }

    #[test]
    fn multi_byte_text_never_panics() {
        let page = "é".repeat(2500);
        assert!(!extract_chunks(&[page]).is_empty());
    }

    fn passage(id: &str, embedding: Vec<f32>) -> Passage {
        Passage {
            chunk: Chunk {
                id: id.to_string(),
                text: id.to_string(),
                page: 1,
            },
            embedding,
        }
    }

    #[test]
    fn search_returns_the_closest_first() {
        let kb = KnowledgeBase {
            source: PDF_FILE.to_string(),
            model_id: DEFAULT_EMBEDDING_MODEL_ID.to_string(),
            dimensions: 2,
            passages: vec![
                passage("far", vec![0.0, 1.0]),
                passage("near", vec![1.0, 0.0]),
                passage("middle", vec![0.7, 0.7]),
            ],
        };
        let ids: Vec<&str> = kb
            .search(&[1.0, 0.0], 2)
            .iter()
            .map(|(_, p)| p.chunk.id.as_str())
            .collect();
        assert_eq!(ids, ["near", "middle"]);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = std::env::temp_dir().join(format!("kb-{}.json", uuid::Uuid::new_v4()));
        let kb = KnowledgeBase {
            source: PDF_FILE.to_string(),
            model_id: DEFAULT_EMBEDDING_MODEL_ID.to_string(),
            dimensions: 1,
            passages: vec![passage("only", vec![1.0])],
        };
        kb.save(&path).unwrap();
        assert_eq!(KnowledgeBase::load(&path).unwrap(), kb);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_file_names_the_build_command() {
        let err = KnowledgeBase::load(Path::new("/nonexistent/kb.json")).unwrap_err();
        assert!(err.to_string().contains("ch5-create-knowledge-base"));
    }
}
