//! Chapter 5 - The Game Master orchestrator (HTTP API on port 8009)
//!
//! A side-by-side port of the Python workshop's
//! `5_a2a_integration/agents/gamemaster_orchestrator/gamemaster_orchestrator.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/5_a2a_integration>
//!
//! The Game Master keeps no rules and no character sheets of its own. It rolls
//! dice over MCP (the Chapter 4 server), and asks the Rules Agent and the
//! Character Agent over A2A, discovering them at runtime with `make_a2a_client`.
//! Every answer comes back as a typed `StoryOutput`.
//!
//! Start the three servers first, each in its own terminal:
//!   cargo run --bin ch4-dice-mcp-server
//!   cargo run --bin ch5-rules-agent
//!   cargo run --bin ch5-character-agent
//! Then:
//!   cargo run --bin ch5-gamemaster-orchestrator
//!   curl -s localhost:8009/inquire -H 'content-type: application/json' -d '{"question":"Roll a d20"}'
//!
//! Environment overrides: MODEL_ID, AWS_REGION, RUST_LOG, MCP_URL (default
//! http://localhost:8002/mcp), CHARACTERS_DB, and GAMEMASTER_BIND (default
//! 127.0.0.1:8009).

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_upon_agentic_ai::{bind_addr, game_master_model};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use strands_agents::a2a::{A2AClientOptions, make_a2a_client};
use strands_agents::tools::mcp::{MCPClient, StreamableHttpTransport};
use strands_agents::tools::structured_output::{
    StructuredOutputTool, parse_structured_output_from_result,
};
use strands_agents::{Agent, AgentBuilder};
use tokio::sync::Mutex;

const DEFAULT_MCP_URL: &str = "http://localhost:8002/mcp";
const RULES_AGENT_URL: &str = "http://127.0.0.1:8000";
const CHARACTER_AGENT_URL: &str = "http://127.0.0.1:8001";
const DEFAULT_PORT: u16 = 8009;

/// The Python chapter's prompt, with TTRPG for the brand name.
const SYSTEM_PROMPT: &str = "You are a TTRPG Game Master. Discover the agents you can reach and ask them instead of guessing: rules questions, character creation and lookups are their job. Every dice roll goes through roll_dice. Never make up what a tool can tell you, and narrate with flair.";

/// Sent once when the model answers in plain text instead of calling
/// `StoryOutput`. Python's `structured_output_model` forces the call; the Rust
/// SDK offers the tool but cannot force it, so we ask a second time.
const STRUCTURED_RETRY: &str =
    "Give that same answer again by calling the StoryOutput tool. Do not call any other tool.";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiceOutput {
    /// The dice type. Ex: d4, d6, d20, etc
    pub dice_type: String,
    /// The dice result value alone
    pub result: i64,
    /// The reason the dice was rolled. Ex: attack roll. And the modificators if there was any
    pub reason: String,
}

/// A single Game Master turn: the narration, what the player could do next, and any dice rolled.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoryOutput {
    /// Your narative response as Game Master
    pub response: String,
    /// ['Action 1', 'Action 2', 'Action 3']
    pub actions_suggestions: Vec<String>,
    /// Brief summary of tools/agents used
    pub details: String,
    /// List of dice rolls with dice_type, result, and reason
    #[serde(default)]
    pub dice_rolls: Vec<DiceOutput>,
}

#[derive(Debug, Deserialize)]
struct QuestionRequest {
    question: String,
}

#[derive(Clone)]
struct AppState {
    // One agent, one conversation: requests take turns, like the Python
    // module-level `agent`.
    agent: Arc<Mutex<Agent>>,
    characters_db: PathBuf,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "healthy" }))
}

async fn messages(State(app): State<AppState>) -> Json<Value> {
    let agent = app.agent.lock().await;
    Json(serde_json::to_value(agent.messages()).unwrap_or(Value::Null))
}

/// Find a character in the Character Agent's TinyDB file, by exact name.
fn find_character(db: &Value, name: &str) -> Option<Value> {
    db.get("_default")?
        .as_object()?
        .values()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
}

/// Reads the Character Agent's file directly, like the Python endpoint.
async fn user(State(app): State<AppState>, Path(user_name): Path<String>) -> Response {
    let db = match tokio::fs::read(&app.characters_db).await {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(db) => db,
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        },
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    match find_character(&db, &user_name) {
        Some(character) => {
            tracing::info!(name = %user_name, "found character");
            Json(character).into_response()
        }
        None => error(
            StatusCode::NOT_FOUND,
            format!("Character with name '{user_name}' not found"),
        ),
    }
}

async fn ask(agent: &mut Agent, question: &str) -> strands_agents::Result<Option<StoryOutput>> {
    let result = agent.run(question).await?;
    Ok(parse_structured_output_from_result(&result).ok())
}

async fn inquire(State(app): State<AppState>, Json(request): Json<QuestionRequest>) -> Response {
    tracing::info!(question = %request.question, "processing request");
    let mut agent = app.agent.lock().await;
    let story = match ask(&mut agent, &request.question).await {
        Ok(Some(story)) => Ok(story),
        Ok(None) => {
            tracing::warn!("no StoryOutput call; asking once more");
            match ask(&mut agent, STRUCTURED_RETRY).await {
                Ok(Some(story)) => Ok(story),
                Ok(None) => Err("the model answered without a StoryOutput twice".to_string()),
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    };
    match story {
        Ok(story) => Json(json!({ "response": story })).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "inquire failed");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "ch5_gamemaster_orchestrator=info,strands_agents::a2a=info".into()
            }),
        )
        .init();

    let mcp_url = std::env::var("MCP_URL").unwrap_or_else(|_| DEFAULT_MCP_URL.to_string());
    let dice = MCPClient::new(Box::new(StreamableHttpTransport::new(mcp_url.as_str())));
    let a2a = make_a2a_client(A2AClientOptions::new([
        RULES_AGENT_URL,
        CHARACTER_AGENT_URL,
    ]))?;

    let agent = AgentBuilder::new()
        .model(Box::new(game_master_model().await?))
        .system_prompt(SYSTEM_PROMPT)
        .tool_provider(Arc::new(dice))
        .tool(Arc::new(a2a))
        .tool(Arc::new(StructuredOutputTool::<StoryOutput>::new()))
        .build()
        .await?;

    let characters_db = std::env::var("CHARACTERS_DB").map_or_else(
        |_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("5_a2a_integration/agents/character_agent/characters.json")
        },
        PathBuf::from,
    );
    let state = AppState {
        agent: Arc::new(Mutex::new(agent)),
        characters_db,
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/messages", get(messages))
        .route("/user/{user_name}", get(user))
        .route("/inquire", post(inquire))
        .with_state(state);

    let addr = bind_addr("GAMEMASTER_BIND", DEFAULT_PORT)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, %mcp_url, "Game Master listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
            tracing::info!("shutting down");
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/5_a2a_integration/agents/character_agent/characters.json"
    );

    #[test]
    fn user_lookup_reads_the_tinydb_file_by_exact_name() {
        let db: Value = serde_json::from_slice(&std::fs::read(SEED).unwrap()).unwrap();
        let arnaud = find_character(&db, "Arnaud").unwrap();
        assert_eq!(arnaud["stats"]["constitution"], 14);
        assert!(find_character(&db, "arnaud").is_none());
    }

    #[test]
    fn story_output_accepts_the_raw_model_shape_without_dice() {
        let raw = json!({
            "response": "The tavern falls silent.",
            "actions_suggestions": ["Approach the bar", "Leave"],
            "details": "No tools used"
        });
        let story = StructuredOutputTool::<StoryOutput>::validate(&raw).unwrap();
        assert!(story.dice_rolls.is_empty());
    }

    #[test]
    fn story_output_rejects_a_text_dice_result() {
        let raw = json!({
            "response": "", "actions_suggestions": [], "details": "",
            "dice_rolls": [{ "dice_type": "d20", "result": "seventeen", "reason": "attack" }]
        });
        assert!(StructuredOutputTool::<StoryOutput>::validate(&raw).is_err());
    }

    #[test]
    fn the_tool_schema_carries_the_field_descriptions() {
        let spec = strands_agents::Tool::spec(&StructuredOutputTool::<StoryOutput>::new());
        let text = serde_json::to_string(&spec).unwrap();
        assert!(text.contains("StoryOutput"));
        assert!(text.contains("Brief summary of tools/agents used"));
    }
}
