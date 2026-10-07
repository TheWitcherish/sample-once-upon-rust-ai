//! Chapter 2 - The Adventurer's Arsenal: Vended Tools
//!
//! A side-by-side port of the Python workshop's `2_vended_tools/`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/2_vended_tools>
//!
//! The game master gains a tool it did not have to write: `http_fetch`, shipped
//! by the `strands-agents-tools` crate. It hits the live Open5e API
//! (<https://api.open5e.com>), which serves the SRD 5.1 - the System Reference
//! Document, a freely licensed (CC-BY-4.0) TTRPG rulebook - to look up real
//! rules instead of inventing them.
//!
//! Run with:
//!   cargo run --bin ch2-vended-tools
//!
//! Environment overrides: same as Chapter 1 (MODEL_ID, AWS_REGION, RUST_LOG).

use std::sync::Arc;

use once_upon_agentic_ai::{GAME_MASTER_PROMPT, game_master_model};
use strands_agents::{AgentBuilder, Result};
use strands_agents_tools::HttpFetch;

/// A spell the SRD actually contains, so the lookup resolves live on stream.
const QUERY: &str = "My wizard wants to cast Fireball. Look up the real SRD rules \
     and tell me its level, casting time, range, and damage.";

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
        .name("Game Master")
        .system_prompt(format!(
            "{GAME_MASTER_PROMPT}. When a rule, spell, or monster detail matters, \
             look it up on the Open5e SRD API with your http_fetch tool - for example \
             https://api.open5e.com/v2/spells/srd_fireball/ - then answer from \
             what you fetched, not from memory."
        ))
        .tool(Arc::new(HttpFetch::new()))
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
