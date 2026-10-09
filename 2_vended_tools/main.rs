//! Chapter 2 - The Adventurer's Arsenal: Vended Tools
//!
//! A side-by-side port of the Python workshop's `2_vended_tools/agent_with_vended_tools.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/2_vended_tools>
//!
//! The game master gains a tool it did not have to write: `http_fetch`, shipped
//! by the `strands-agents-tools` crate (Python: `http_request` from
//! `strands.vended_tools`). It looks up real spell and monster data on the live
//! 5e SRD API instead of inventing it.
//!
//! Run with:
//!   cargo run --bin ch2-vended-tools
//!
//! Environment overrides: same as Chapter 1 (MODEL_ID, AWS_REGION, RUST_LOG).

use std::sync::Arc;

use once_upon_agentic_ai::{GAME_MASTER_PROMPT, game_master_model};
use strands_agents::{AgentBuilder, Result};
// TODO: Step 1 - Import HttpFetch from strands_agents_tools

/// Where the rules live. Kept line for line with the Python prompt so the two
/// versions read side by side.
const RULES_LOOKUP: &str =
    "When asked about a spell or a monster, look it up on the 5e SRD API and answer from the data:
    - spells:   https://www.dnd5eapi.co/api/2014/spells/<index>
    - monsters: https://www.dnd5eapi.co/api/2014/monsters/<index>
    where <index> is the lowercase name with hyphens (e.g. fireball, adult-red-dragon).";

/// The Python chapter's exact query.
const QUERY: &str = "What does the Fireball spell do, and how much damage does it deal?";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "strands_agents=info".into()),
        )
        .init();

    let model = game_master_model().await?;

    // The agent did not write this tool - the SDK vends it. We hand it over and
    // tell the game master where the rules live; it decides when to reach for it.
    let mut agent = AgentBuilder::new()
        .model(Box::new(model))
        // TODO: Step 1 - Add the HttpFetch tool to your agent
        .system_prompt(format!("{GAME_MASTER_PROMPT}.\n    {RULES_LOOKUP}"))
        .build()
        .await?;

    println!("Adventurer: {QUERY}\n");
    let result = agent.run(QUERY).await?;
    println!("Game Master: {}\n", result.content);

    let usage = &result.event_loop_metrics.accumulated_usage;
    println!(
        "[stop_reason={} | input_tokens={} | output_tokens={}]",
        result.stop_reason, usage.input_tokens, usage.output_tokens
    );

    Ok(())
}
