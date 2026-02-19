use std::sync::Arc;

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Serialize;
use tracing::info;

use crate::registry::{CommandEntry, RegistryError};
use crate::AppState;

/// Command list response
#[derive(Serialize)]
struct CommandListResponse {
    commands: Vec<CommandEntry>,
    total: usize,
}

/// Status response
#[derive(Serialize)]
struct StatusResponse {
    last_updated: Option<String>,
    command_count: usize,
    error_count: usize,
    errors: Vec<RegistryError>,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// Success response
#[derive(Serialize)]
struct SuccessResponse {
    success: bool,
    message: String,
}

/// Create the API router
pub fn api_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/commands", get(list_commands))
        .route("/api/status", get(get_status))
        .route("/api/health", get(health))
        .route("/api/reload", post(force_reload))
}

/// GET /api/commands - List all commands
async fn list_commands(State(state): State<Arc<AppState>>) -> Json<CommandListResponse> {
    let reg = state.registry_state.read().await;
    let total = reg.entries.len();
    Json(CommandListResponse {
        commands: reg.entries.clone(),
        total,
    })
}

/// GET /api/status - Get registry status
async fn get_status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let reg = state.registry_state.read().await;
    Json(StatusResponse {
        last_updated: Some(reg.timestamp.to_rfc3339()),
        command_count: reg.entries.len(),
        error_count: reg.errors.len(),
        errors: reg.errors.clone(),
    })
}

/// GET /api/health - Health check
async fn health() -> Json<HealthResponse> {
    let version = option_env!("GIT_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    Json(HealthResponse {
        status: "ok".to_string(),
        version: version.to_string(),
    })
}

/// POST /api/reload - Force a full rebuild
async fn force_reload(State(state): State<Arc<AppState>>) -> Json<SuccessResponse> {
    info!("Manual reload requested");

    if let Some(ref tx) = *state.rebuild_tx.read().await {
        match tx.send(()).await {
            Ok(_) => Json(SuccessResponse {
                success: true,
                message: "Rebuild triggered".to_string(),
            }),
            Err(e) => Json(SuccessResponse {
                success: false,
                message: format!("Failed to trigger rebuild: {}", e),
            }),
        }
    } else {
        Json(SuccessResponse {
            success: false,
            message: "Rebuild channel not available".to_string(),
        })
    }
}
