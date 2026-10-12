//! Chapter 1 - The Art of Agent Summoning
//!
//! A side-by-side port of the Python workshop's `1_strands_basics/simple_agent.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/1_strands_basics>
//!
//! Three TODOs, the same three steps as the Python chapter.
//!
//! Run with:
//!   cargo run --bin ch1-summoning
//!
//! Environment overrides:
//!   MODEL_ID     a Bedrock inference-profile id (default: Claude Sonnet 4.6)
//!   AWS_REGION   the region your model lives in, e.g. us-east-1
//!   RUST_LOG     set to strands_agents=debug for the agent's inner monologue

use once_upon_agentic_ai::{GAME_MASTER_PROMPT, game_master_model, model_id};
use strands_agents::{AgentBuilder, Result};

/// The Python chapter's exact first query.
const FIRST_QUERY: &str = "Hi, I am an adventurer ready for adventure!";

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: Step 1 - Add debug logging to see what your agent is thinking

    println!("Summoning a game master on {}\n", model_id());

    // TODO: Step 2 - Create the agent with the following system prompt: "You are a Game Master for a Table-Top Role-Playing Game (TTRPG)"

    // TODO: Step 3 - Invoke your agent with a basic query such as "Hi, I am an adventurer ready for adventure!"

    Ok(())
}
