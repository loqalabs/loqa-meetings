use super::handlers;
use super::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;

/// Create the HTTP router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // Recording control
        .route("/meetings/record/start", post(handlers::start_recording))
        .route(
            "/meetings/record/stop/:meeting_id",
            post(handlers::stop_recording),
        )
        // Meeting queries
        .route(
            "/meetings/:meeting_id/status",
            get(handlers::get_meeting_status),
        )
        .route(
            "/meetings/:meeting_id/transcript",
            get(handlers::get_meeting_transcript),
        )
        // Add tracing middleware for request logging
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
