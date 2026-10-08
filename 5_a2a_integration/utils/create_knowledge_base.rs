//! Chapter 5 - Build the Rules Agent knowledge base
//!
//! A side-by-side port of the Python workshop's
//! `5_a2a_integration/utils/create_knowledge_base.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/5_a2a_integration>
//!
//! Reads `DnD_BasicRules_2018.pdf` page by page, cuts each page into
//! overlapping ~1000-character passages, embeds every passage with Titan Text
//! Embeddings V2 on Bedrock, and writes `rules_knowledge_base.json` next to the
//! PDF. Run it once, with the PDF in `5_a2a_integration/utils/`:
//!   cargo run --release --bin ch5-create-knowledge-base
//!
//! Environment overrides: EMBEDDING_MODEL_ID, AWS_REGION, AWS_PROFILE.

use once_upon_agentic_ai::knowledge_base::{
    EMBEDDING_DIMENSIONS, Embedder, KbResult, KnowledgeBase, PDF_FILE, Passage, extract_chunks,
    kb_path, pdf_path,
};
use tokio::task::JoinSet;

/// Embedding calls in flight at once. Enough to finish in about a minute,
/// few enough to stay well under the Bedrock request quota.
const CONCURRENCY: usize = 10;
const PROGRESS_EVERY: usize = 100;

async fn create_knowledge_base() -> KbResult<()> {
    let pdf = pdf_path();
    if !pdf.exists() {
        return Err(format!(
            "'{}' not found. Put {PDF_FILE} in 5_a2a_integration/utils/ first (see the workshop instructions).",
            pdf.display()
        )
        .into());
    }

    println!("Extracting text from the PDF...");
    let pages = pdf_extract::extract_text_by_pages(&pdf)?;
    let chunks = extract_chunks(&pages);
    println!(
        "{} passages extracted from {} pages",
        chunks.len(),
        pages.len()
    );

    let embedder = Embedder::from_env().await;
    println!(
        "Embedding passages with {} ({EMBEDDING_DIMENSIONS} dimensions)...",
        embedder.model_id()
    );
    let mut passages = Vec::with_capacity(chunks.len());
    let mut next_report = PROGRESS_EVERY;
    for group in chunks.chunks(CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for (slot, chunk) in group.iter().cloned().enumerate() {
            let embedder = embedder.clone();
            tasks.spawn(async move {
                let embedding = embedder.embed(&chunk.text).await;
                (slot, chunk, embedding)
            });
        }
        // Tasks finish in any order; put them back in PDF order.
        let mut done = Vec::with_capacity(group.len());
        while let Some(joined) = tasks.join_next().await {
            let (slot, chunk, embedding) = joined?;
            done.push((
                slot,
                Passage {
                    chunk,
                    embedding: embedding?,
                },
            ));
        }
        done.sort_by_key(|(slot, _)| *slot);
        passages.extend(done.into_iter().map(|(_, p)| p));

        if passages.len() >= next_report || passages.len() == chunks.len() {
            println!("  {}/{}", passages.len(), chunks.len());
            next_report += PROGRESS_EVERY;
        }
    }

    let kb = KnowledgeBase {
        source: PDF_FILE.to_string(),
        model_id: embedder.model_id().to_string(),
        dimensions: EMBEDDING_DIMENSIONS,
        passages,
    };
    let path = kb_path();
    kb.save(&path)?;
    println!(
        "Knowledge base created at {} ({} passages)",
        path.display(),
        kb.passages.len()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> KbResult<()> {
    create_knowledge_base().await?;
    println!("Knowledge base creation complete!");
    Ok(())
}
