//! Chapter 3 - The Art of Magical Forging: the Dice Roller
//!
//! A side-by-side port of the Python workshop's `3_custom_tools/`:
//!   <https://github.com/aws-samples/sample-once-upon-agentic-ai/tree/main/3_custom_tools>
//!
//! Chapter 2 handed the agent a tool someone else wrote. Here we forge our own:
//! a plain `roll_dice` function, wrapped in the SDK's `Tool` trait so the model
//! can see it, call it, and get a real random number back instead of inventing one.
//!
//! Run with:
//!   cargo run --bin ch3-custom-tools
//!
//! Environment overrides: same as Chapter 1 (MODEL_ID, AWS_REGION, RUST_LOG).

use std::sync::Arc;

use async_trait::async_trait;
use once_upon_agentic_ai::game_master_model;
use serde_json::{Value, json};
use strands_agents::tools::ToolResult;
use strands_agents::{AgentBuilder, Result, Tool, ToolContext, ToolSpec};

const LADY_LUCK_PROMPT: &str = "You are Lady Luck, the mystical keeper of dice and fortune in \
Table-Top Role-Playing Game (TTRPG) adventures. \
You speak with theatrical flair and always announce dice rolls with appropriate drama. \
You know all about TTRPG mechanics, ability scores, and can help players with character creation. \
When rolling ability scores, remember the traditional method: roll 4d6, drop the lowest die.";

const QUERY: &str = "Help me create a new TTRPG character! Roll the strength, wisdom, charisma \
and intelligence abilities scores using 4d6 drop lowest method.";

/// Roll a single die with the given number of faces.
///
/// The same function as the Python workshop's `roll_dice`, minus the decorator:
/// it knows nothing about agents, so it can be tested on its own.
fn roll_dice(faces: i64) -> std::result::Result<i64, String> {
    if faces < 1 {
        return Err("Dice must have at least 1 face".to_string());
    }
    Ok(rand::random_range(1..=faces))
}

/// The bridge between our function and the model.
///
/// Python's `@tool` reads the signature and docstring to build this for you.
/// In Rust we write the spec by hand: name, description, and a JSON schema the
/// model fills in when it decides to roll.
struct RollDice;

#[async_trait]
impl Tool for RollDice {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "roll_dice",
            "Roll a single die and return the result. \
             Use it for every dice roll; never invent a number.",
            json!({
                "type": "object",
                "properties": {
                    "faces": {
                        "type": "integer",
                        "description": "Number of faces on the die, e.g. 6 for a d6 or 20 for a d20",
                        "default": 6
                    }
                }
            }),
        )
    }

    async fn execute(&self, input: Value, _ctx: &mut ToolContext) -> Result<ToolResult> {
        // The model sends raw JSON. A missing `faces` means the default d6;
        // anything that is not an integer goes back as an error the model can read.
        let faces = match input.get("faces") {
            None => 6,
            Some(value) => match value.as_i64() {
                Some(faces) => faces,
                None => {
                    return Ok(ToolResult::error(format!(
                        "faces must be an integer, got {value}"
                    )));
                }
            },
        };

        Ok(match roll_dice(faces) {
            Ok(roll) => {
                tracing::info!(faces, roll, "roll_dice");
                ToolResult::success(roll.to_string())
            }
            Err(reason) => ToolResult::error(reason),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ch3_custom_tools=info".into()),
        )
        .init();

    // What the SDK tells the model about our tool - the Rust twin of
    // Python's `print(roll_dice.tool_spec)`.
    println!("{}\n", serde_json::to_string_pretty(&RollDice.spec())?);

    let model = game_master_model().await?;

    let mut dice_master = AgentBuilder::new()
        .model(Box::new(model))
        .name("Lady Luck")
        .system_prompt(LADY_LUCK_PROMPT)
        .tool(Arc::new(RollDice))
        .build()
        .await?;

    println!("Adventurer: {QUERY}\n");
    let result = dice_master.run(QUERY).await?;
    println!("Lady Luck: {}\n", result.content);

    // 4 abilities x 4d6 should mean 16 real rolls. Count them.
    let metrics = &result.event_loop_metrics;
    let rolls = metrics
        .tool_metrics
        .get("roll_dice")
        .map_or(0, |m| m.call_count);
    println!(
        "[roll_dice calls={rolls} | stop_reason={} | input_tokens={} | output_tokens={}]",
        result.stop_reason,
        metrics.accumulated_usage.input_tokens,
        metrics.accumulated_usage.output_tokens
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use strands_agents::tools::ToolStatus;

    #[test]
    fn roll_dice_stays_on_the_die() {
        for _ in 0..1_000 {
            let roll = roll_dice(20).expect("a d20 is a valid die");
            assert!((1..=20).contains(&roll));
        }
    }

    #[test]
    fn roll_dice_refuses_a_die_with_no_faces() {
        assert!(roll_dice(0).is_err());
        assert!(roll_dice(-3).is_err());
    }

    // These go through `execute` with the raw JSON shape the model produces,
    // not through `roll_dice` directly, so the boundary itself is tested.
    async fn call(input: Value) -> ToolResult {
        RollDice
            .execute(input, &mut ToolContext::default())
            .await
            .expect("execute never returns Err")
    }

    #[tokio::test]
    async fn missing_faces_rolls_a_d6() {
        let result = call(json!({})).await;
        assert!(matches!(result.status, ToolStatus::Success));
    }

    #[tokio::test]
    async fn zero_faces_comes_back_as_a_tool_error() {
        let result = call(json!({ "faces": 0 })).await;
        assert!(matches!(result.status, ToolStatus::Error));
    }

    #[tokio::test]
    async fn non_integer_faces_comes_back_as_a_tool_error() {
        let result = call(json!({ "faces": "twenty" })).await;
        assert!(matches!(result.status, ToolStatus::Error));
    }
}
