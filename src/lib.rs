//! Shared helpers for the *Once Upon Agentic AI* (Rust) workshop.
//!
//! Every chapter talks to the same storyteller model. Rather than repeat the
//! environment-variable dance in each chapter, the model is built here once and
//! each chapter stays focused on the one concept it teaches.

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use strands_agents::Result;
use strands_agents::a2a::StrandsA2AExecutor;
use strands_agents::a2a::server::rpc_dispatch;
use strands_agents::models::{BedrockModel, ModelConfig};

pub mod knowledge_base;

/// Default Bedrock model for the whole adventure.
///
/// Verified live against the configured Bedrock account (us-east-1, 2026-10-05
/// via `aws bedrock list-inference-profiles`): `ACTIVE`, and the same model the
/// Python workshop teaches against. Override it with `MODEL_ID` for a cheaper
/// run, e.g. `export MODEL_ID=us.anthropic.claude-haiku-4-5-20251001-v1:0`.
pub const DEFAULT_MODEL_ID: &str = "us.anthropic.claude-sonnet-4-6";

/// The system prompt shared by the whole adventure.
pub const GAME_MASTER_PROMPT: &str =
    "You are a Game Master for a Table-Top Role-Playing Game (TTRPG)";

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
    let config = ModelConfig::new(model_id()).with_temperature(0.9);
    BedrockModel::with_config(config).await
}

/// Resolve where an A2A server listens: `env_var` if set, else loopback on
/// `default_port`. Loopback by default because these servers have no
/// authentication, the same as the Python chapter.
///
/// # Errors
///
/// Returns the parse error when `env_var` holds something that is not
/// `host:port`.
pub fn bind_addr(
    env_var: &str,
    default_port: u16,
) -> std::result::Result<SocketAddr, std::net::AddrParseError> {
    match env::var(env_var) {
        Ok(addr) => addr.parse(),
        Err(_) => Ok(SocketAddr::from(([127, 0, 0, 1], default_port))),
    }
}

#[derive(Clone)]
struct A2AState {
    executor: Arc<StrandsA2AExecutor>,
    public_url: String,
}

async fn agent_card(State(app): State<A2AState>) -> Json<Value> {
    Json(
        serde_json::to_value(app.executor.agent_card(&app.public_url))
            .unwrap_or_else(|e| json!({ "error": e.to_string() })),
    )
}

async fn rpc(State(app): State<A2AState>, Json(body): Json<Value>) -> Json<Value> {
    let response = rpc_dispatch(&app.executor, body).await;
    Json(serde_json::to_value(response).unwrap_or_else(|e| {
        json!({ "jsonrpc": "2.0", "id": null,
                "error": { "code": -32603, "message": e.to_string() } })
    }))
}

/// Serve an agent over A2A until Ctrl-C: the Rust side of Python's
/// `A2AServer(agent=..., port=...).serve()`.
///
/// Routes:
/// - `GET /.well-known/agent-card.json`: how other agents discover this one.
/// - `POST /`: JSON-RPC 2.0 (`message/send`, `tasks/get`, ...). Root, like the
///   Python server, because that is where `A2AAgent` and `make_a2a_client` post.
///
/// # Errors
///
/// Returns the I/O error when the address cannot be bound (for example, the
/// port is already taken) or the server stops unexpectedly.
pub async fn serve_a2a(executor: StrandsA2AExecutor, addr: SocketAddr) -> std::io::Result<()> {
    let public_url = format!("http://{addr}");
    let state = A2AState {
        executor: Arc::new(executor),
        public_url: public_url.clone(),
    };
    let app = Router::new()
        .route("/.well-known/agent-card.json", get(agent_card))
        .route("/", post(rpc))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%public_url, "A2A server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            // If the signal handler cannot be installed, keep serving rather
            // than exit at once; the process can still be killed.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
            tracing::info!("shutting down");
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_addr_defaults_to_loopback() {
        let addr = bind_addr("ONCE_UPON_TEST_UNSET_BIND", 8000);
        assert_eq!(addr, Ok(SocketAddr::from(([127, 0, 0, 1], 8000))));
    }

    #[test]
    fn bind_addr_rejects_garbage() {
        // SAFETY: this test is the only reader and writer of this variable.
        unsafe { env::set_var("ONCE_UPON_TEST_BAD_BIND", "not-an-address") };
        assert!(bind_addr("ONCE_UPON_TEST_BAD_BIND", 8000).is_err());
    }
}
