//! HTTP API server for external control (Obsidian plugin)
//!
//! This module provides a REST API for controlling recording sessions:
//! - POST /meetings/record/start - Start a new recording
//! - POST /meetings/record/stop/:id - Stop a recording
//! - GET /meetings/:id/status - Query session status
//! - GET /meetings/:id/transcript - Get accumulated transcript
//! - GET /health - Health check

mod handlers;
mod routes;
mod state;

pub use routes::create_router;
pub use state::AppState;
