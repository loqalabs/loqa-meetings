use anyhow::Result;
use loqa_meetings::{create_router, AppState};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🎙️  Loqa Meetings v0.1.0 - HTTP API Server");

    // Create application state
    let app_state = AppState::new();

    // Create HTTP router
    let app = create_router(app_state);

    // Start HTTP server
    let addr = "127.0.0.1:3000";
    info!("🌐 Starting HTTP server on http://{}", addr);
    info!("📋 API endpoints:");
    info!("   POST   /meetings/record/start");
    info!("   POST   /meetings/record/stop/:meeting_id");
    info!("   GET    /meetings/:meeting_id/status");
    info!("   GET    /meetings/:meeting_id/transcript");
    info!("   GET    /health");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
