//! Shared helpers for the *Once Upon Agentic AI* (Rust) workshop.
//!
//! Every chapter talks to the same storyteller model. Rather than repeat the
//! environment-variable dance in each chapter, the model is built here once and
//! each chapter stays focused on the one concept it teaches.

use std::env;

use strands_agents::Result;
use strands_agents::models::{BedrockModel, ModelConfig};

/// Default Bedrock model for the whole adventure.
///
/// Verified live against the configured Bedrock account (us-east-1, 2026-10-05
/// via `aws bedrock list-inference-profiles`): `ACTIVE`, and the same model the
/// Python workshop teaches against. Override it with `MODEL_ID` for a cheaper
/// run, e.g. `export MODEL_ID=us.anthropic.claude-haiku-4-5-20251001-v1:0`.
pub const DEFAULT_MODEL_ID: &str = "us.anthropic.claude-sonnet-4-6";

/// The system prompt shared by the whole adventure.
///
/// Kept verbatim from the Python workshop (typo and all) so the two versions
/// read side by side.
pub const GAME_MASTER_PROMPT: &str = "You are a game master for a Dungeon & Dragon game";

/// Resolve the model id from `MODEL_ID`, falling back to the verified default.
pub fn model_id() -> String {
    env::var("MODEL_ID").unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string())
}

/// Build the storyteller model on Amazon Bedrock.
///
/// Honours `MODEL_ID` and the standard AWS environment (`AWS_REGION`,
/// `AWS_PROFILE`, ...). A high temperature is the right default for a
/// storyteller; a low one would make it repeat itself.
pub async fn game_master_model() -> Result<BedrockModel> {
    let config = ModelConfig::new(model_id())
        .with_temperature(0.9)
        .with_max_tokens(1024);
    BedrockModel::with_config(config).await
}
