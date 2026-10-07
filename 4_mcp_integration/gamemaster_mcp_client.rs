//! Chapter 4 - Planar Portals: Lady Luck rolls through MCP
//!
//! A side-by-side port of the Python workshop's `4_mcp_integration/gamemaster_mcp_client.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/4_mcp_integration>
//!
//! Lady Luck has no dice of her own any more. She connects to the dice roller
//! MCP server over streamable HTTP, discovers `roll_dice` at runtime, and calls
//! it across the wire. Same SDK, same agent; the tool just lives somewhere else.
//!
//! Start the server first, in another terminal:
//!   cargo run --bin ch4-dice-mcp-server
//! Then:
//!   cargo run --bin ch4-gamemaster-mcp-client
//!
//! Environment overrides: same as Chapter 1, plus
//!   MCP_URL  the server's MCP endpoint (default http://localhost:8002/mcp)

use std::env;
use std::sync::Arc;

use once_upon_agentic_ai::game_master_model;
use strands_agents::tools::mcp::{MCPClient, StreamableHttpTransport};
use strands_agents::{AgentBuilder, Result};

const DEFAULT_MCP_URL: &str = "http://localhost:8002/mcp";

/// The Python chapter's prompt, line for line, with TTRPG for the brand name.
const LADY_LUCK_PROMPT: &str = "You are Lady Luck, the mystical keeper of dice and fortune in TTRPG adventures.
    You speak with theatrical flair and always announce dice rolls with appropriate drama.
    You know all about TTRPG mechanics, always use the appropriate tools when applicable - never make up results!";

/// The Python chapter's query, with TTRPG for the brand name.
const QUERY: &str = "Help me create a new TTRPG character! Roll the strength, wisdom, charisma and intelligence abilities scores using 4d6 drop lowest method.";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "strands_agents=warn".into()),
        )
        .init();

    let url = env::var("MCP_URL").unwrap_or_else(|_| DEFAULT_MCP_URL.to_string());
    let dice_portal = MCPClient::new(Box::new(StreamableHttpTransport::new(url.as_str())));

    let model = game_master_model().await?;
    let mut gamemaster = AgentBuilder::new()
        .model(Box::new(model))
        .system_prompt(LADY_LUCK_PROMPT)
        .tool_provider(Arc::new(dice_portal))
        .build()
        .await?;

    println!("Adventurer: {QUERY}\n");
    let result = gamemaster.run(QUERY).await?;
    println!("Lady Luck: {}\n", result.content);

    // The rolls happened in another process. Count how many crossed the portal;
    // the server's log shows each one.
    let metrics = &result.event_loop_metrics;
    let rolls = metrics
        .tool_metrics
        .get("roll_dice")
        .map_or(0, |m| m.call_count);
    println!(
        "[roll_dice calls over MCP={rolls} | server={url} | stop_reason={}]",
        result.stop_reason
    );

    Ok(())
}
