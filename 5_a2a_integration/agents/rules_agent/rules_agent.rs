//! Chapter 5 - The Rules Agent (A2A server on port 8000)
//!
//! A side-by-side port of the Python workshop's
//! `5_a2a_integration/agents/rules_agent/rules_agent.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/5_a2a_integration>
//!
//! One tool, `query_rules`: a semantic search over the Basic Rules PDF that
//! returns the three closest passages with their page numbers. Served over A2A
//! so the Game Master can ask this agent instead of guessing the rules.
//!
//! Build the knowledge base once, then run the agent:
//!   cargo run --release --bin ch5-create-knowledge-base
//!   cargo run --bin ch5-rules-agent
//!
//! Environment overrides: MODEL_ID, EMBEDDING_MODEL_ID, AWS_REGION, RUST_LOG,
//! plus RULES_AGENT_BIND (default 127.0.0.1:8000).

use std::sync::Arc;

use async_trait::async_trait;
use once_upon_agentic_ai::knowledge_base::{Embedder, KbResult, KnowledgeBase, Passage, kb_path};
use once_upon_agentic_ai::{bind_addr, game_master_model, serve_a2a};
use serde_json::{Value, json};
use strands_agents::a2a::{AgentSkill, StrandsA2AExecutor};
use strands_agents::tools::ToolResult;
use strands_agents::{AgentBuilder, Tool, ToolContext, ToolSpec};

const NAME: &str = "Rules Agent";
const DESCRIPTION: &str =
    "5e rules lookup: fast, page-referenced answers from the Basic Rules knowledge base.";
const SYSTEM_PROMPT: &str = "You are a 5e rules expert. For each rules question, call query_rules once, then answer briefly with the page reference.";

const DEFAULT_PORT: u16 = 8000;
const TOP_K: usize = 3;

/// The same layout as the Python tool: `[Page N] passage`, blank-line separated.
fn format_passages(hits: &[(f32, &Passage)]) -> String {
    hits.iter()
        .map(|(_, p)| format!("[Page {}] {}", p.chunk.page, p.chunk.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

struct QueryRules {
    kb: KnowledgeBase,
    embedder: Embedder,
}

impl QueryRules {
    async fn lookup(&self, query: &str) -> KbResult<String> {
        let vector = self.embedder.embed(query).await?;
        let hits = self.kb.search(&vector, TOP_K);
        for (score, p) in &hits {
            tracing::info!(page = p.chunk.page, score = format!("{score:.3}"), "match");
        }
        Ok(format_passages(&hits))
    }
}

#[async_trait]
impl Tool for QueryRules {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "query_rules",
            "Look up a 5e rule in the Basic Rules knowledge base.\n\n\
             Use it for any question about game mechanics: ability checks, combat, \
             spellcasting, conditions, resting. Ask in plain English, as a player would. \
             The lookup is a semantic search over the Basic Rules PDF: the three \
             passages closest to the question come back, each with its page number.\n\n\
             Example response:\n    \
             \"[Page 74] When a hostile creature that you can see moves out of your reach, ...\n\n    \
             [Page 73] ...\"\n\n\
             Notes:\n    \
             - Passages are about 1,000 characters long; the answer is usually in the first one.\n    \
             - Fails if the knowledge base has not been built yet (cargo run --bin ch5-create-knowledge-base).\n\n\
             Returns the matching passages, each prefixed with its page reference, separated by blank lines.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The rules question or topic, in plain English, e.g. \"opportunity attack\" or \"what are the rules for dexterity checks\"."
                    }
                },
                "required": ["query"]
            }),
        )
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &mut ToolContext,
    ) -> strands_agents::Result<ToolResult> {
        let Some(query) = input.get("query").and_then(Value::as_str) else {
            return Ok(ToolResult::error("query must be a string"));
        };
        tracing::info!(query, "query_rules");
        Ok(match self.lookup(query).await {
            Ok(text) => ToolResult::success(text),
            Err(e) => ToolResult::error(e.to_string()),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ch5_rules_agent=info,once_upon_agentic_ai=info".into()),
        )
        .init();

    // Python opens ChromaDB on the first query; loading at startup fails
    // before the server is advertised, not in the middle of a game.
    let kb = KnowledgeBase::load(&kb_path()).map_err(|e| e.to_string())?;
    let embedder = Embedder::from_env().await;
    // Vectors from two embedding models live in different spaces: comparing
    // them returns confident nonsense, so refuse instead.
    if kb.model_id != embedder.model_id() {
        return Err(format!(
            "knowledge base was embedded with {}, but EMBEDDING_MODEL_ID is {}; rebuild it",
            kb.model_id,
            embedder.model_id()
        )
        .into());
    }
    tracing::info!(passages = kb.passages.len(), model = %kb.model_id, "knowledge base loaded");

    let agent = AgentBuilder::new()
        .model(Box::new(game_master_model().await?))
        // TODO: Step 1 - Add the query_rules tool to the agent
        // TODO: Step 2 - Add the name NAME ("Rules Agent") to the agent
        // TODO: Step 3 - Add the description DESCRIPTION ("5e rules lookup: fast, page-referenced answers from the Basic Rules knowledge base.") to the agent
        .system_prompt(SYSTEM_PROMPT)
        .build()
        .await?;

    // TODO: Step 4 - Create a StrandsA2AExecutor for the agent, with query_rules as a skill

    // TODO: Step 5 - Start the A2A server on port 8000 with serve_a2a
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_upon_agentic_ai::knowledge_base::Chunk;

    fn passage(page: usize, text: &str) -> Passage {
        Passage {
            chunk: Chunk {
                id: format!("page_{page}_offset_0"),
                text: text.to_string(),
                page,
            },
            embedding: Vec::new(),
        }
    }

    #[test]
    fn passages_are_page_referenced_and_blank_line_separated() {
        let a = passage(74, "Opportunity attacks.");
        let b = passage(73, "Movement.");
        assert_eq!(
            format_passages(&[(0.9, &a), (0.5, &b)]),
            "[Page 74] Opportunity attacks.\n\n[Page 73] Movement."
        );
    }

    #[tokio::test]
    async fn a_query_that_is_not_text_is_a_tool_error() {
        let tool = QueryRules {
            kb: KnowledgeBase {
                source: String::new(),
                model_id: String::new(),
                dimensions: 0,
                passages: Vec::new(),
            },
            embedder: Embedder::from_env().await,
        };
        let result = tool
            .execute(json!({ "query": 42 }), &mut ToolContext::default())
            .await
            .unwrap();
        assert!(matches!(
            result.status,
            strands_agents::tools::ToolStatus::Error
        ));
    }
}
