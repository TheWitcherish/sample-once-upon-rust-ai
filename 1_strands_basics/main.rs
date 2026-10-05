//! Chapter 1 - The Art of Agent Summoning
//!
//! A side-by-side port of the Python workshop's `1_strands_basics/simple_agent.py`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/1_strands_basics>
//!
//! The Python chapter has three TODOs; each maps to a step below:
//!   1. Add debug logging to see what your agent is thinking.
//!   2. Create the agent with the game-master system prompt.
//!   3. Invoke the agent with "Hi, I am an adventurer ready for adventure!".
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
    // Step 1: debug logging - see what your agent is thinking.
    //
    // Python: logging.getLogger("strands").setLevel(logging.DEBUG)
    // Rust:   the SDK emits `tracing` spans on the `strands_agents` target, so a
    //         subscriber with an env filter is the direct analogue. Default to
    //         info so the demo stays readable; RUST_LOG=strands_agents=debug
    //         opens the floodgates.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "strands_agents=info".into()),
        )
        .with_target(true)
        .init();

    println!("Summoning a game master on {}\n", model_id());

    // Step 2: create the agent with the system prompt.
    //
    // Python's Agent() defaults to Bedrock. Rust names the provider explicitly -
    // that is the only structural difference in this whole chapter.
    let model = game_master_model().await?;

    let mut agent = AgentBuilder::new()
        .model(Box::new(model))
        .name("Game Master")
        .system_prompt(GAME_MASTER_PROMPT)
        .build()
        .await?;

    // Step 3: invoke the agent.
    //
    // Python: agent("Hi, ...") prints as a side effect.
    // Rust:   agent.run(..) returns an AgentResult, so the caller decides what
    //         to do with it.
    println!("Adventurer: {FIRST_QUERY}\n");
    let result = agent.run(FIRST_QUERY).await?;
    println!("Game Master: {}\n", result.content);

    let usage = &result.event_loop_metrics.accumulated_usage;
    println!(
        "[stop_reason={} | input_tokens={} | output_tokens={}]\n",
        result.stop_reason, usage.input_tokens, usage.output_tokens
    );

    // The agent carries its own history, so a follow-up needs no plumbing.
    let follow_up = "I check the tavern for a quest board. What do I find?";
    println!("Adventurer: {follow_up}\n");
    let result = agent.run(follow_up).await?;
    println!("Game Master: {}\n", result.content);

    println!("Messages retained in history: {}\n", agent.messages().len());

    Ok(())
}
