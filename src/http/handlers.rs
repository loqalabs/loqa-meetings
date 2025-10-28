use super::state::AppState;
use crate::session::{RecordingSession, SessionConfig, SessionStats, TranscriptSegment};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct StartRecordingRequest {
    /// Optional meeting ID (if not provided, generate UUID)
    pub meeting_id: Option<String>,

    /// Optional meeting title
    pub title: Option<String>,

    /// Chunk duration in seconds (default: 300 = 5 minutes)
    pub chunk_duration_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct StartRecordingResponse {
    pub meeting_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StopRecordingResponse {
    pub meeting_id: String,
    pub status: String,
    pub message: String,
    pub stats: SessionStats,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /meetings/record/start
/// Start a new recording session
pub async fn start_recording(
    State(state): State<AppState>,
    Json(req): Json<StartRecordingRequest>,
) -> impl IntoResponse {
    // Generate or use provided meeting ID
    let meeting_id = req
        .meeting_id
        .unwrap_or_else(|| format!("meeting-{}", uuid::Uuid::new_v4()));

    info!("Starting recording for meeting: {}", meeting_id);

    // Check if already recording
    {
        let sessions = state.sessions.read().await;
        if sessions.contains_key(&meeting_id) {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!("Meeting {} is already recording", meeting_id),
                }),
            )
                .into_response();
        }
    }

    // Create session config
    let config = SessionConfig {
        session_id: meeting_id.clone(),
        chunk_duration: std::time::Duration::from_secs(req.chunk_duration_secs.unwrap_or(300)),
        sample_rate: 16000,                            // Whisper expects 16kHz
        channels: 1,                                   // Mono
        nats_url: "nats://localhost:4222".to_string(), // TODO: Make configurable
    };

    // Create recording session
    let session = match RecordingSession::new(config).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!("Failed to create session: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to create session: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Start recording
    if let Err(e) = session.start().await {
        error!("Failed to start recording: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to start recording: {}", e),
            }),
        )
            .into_response();
    }

    // Store session
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(meeting_id.clone(), session);
    }

    info!("Recording started successfully for meeting: {}", meeting_id);

    (
        StatusCode::OK,
        Json(StartRecordingResponse {
            meeting_id: meeting_id.clone(),
            status: "recording".to_string(),
            message: format!("Recording started for meeting {}", meeting_id),
        }),
    )
        .into_response()
}

/// POST /meetings/record/stop/:meeting_id
/// Stop recording for a specific meeting
pub async fn stop_recording(
    State(state): State<AppState>,
    Path(meeting_id): Path<String>,
) -> impl IntoResponse {
    info!("Stopping recording for meeting: {}", meeting_id);

    // Find and remove session
    let session = {
        let mut sessions = state.sessions.write().await;
        sessions.remove(&meeting_id)
    };

    match session {
        Some(session) => {
            // Stop recording
            match session.stop().await {
                Ok(stats) => {
                    info!("Recording stopped successfully for meeting: {}", meeting_id);
                    (
                        StatusCode::OK,
                        Json(StopRecordingResponse {
                            meeting_id: meeting_id.clone(),
                            status: "stopped".to_string(),
                            message: "Recording stopped".to_string(),
                            stats,
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    error!("Failed to stop recording: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to stop recording: {}", e),
                        }),
                    )
                        .into_response()
                }
            }
        }
        None => {
            error!("Meeting {} not found", meeting_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Meeting {} not found", meeting_id),
                }),
            )
                .into_response()
        }
    }
}

/// GET /meetings/:meeting_id/status
/// Get status of a recording session
pub async fn get_meeting_status(
    State(state): State<AppState>,
    Path(meeting_id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.sessions.read().await;

    match sessions.get(&meeting_id) {
        Some(session) => match session.get_stats().await {
            Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
            Err(e) => {
                error!("Failed to get stats: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to get stats: {}", e),
                    }),
                )
                    .into_response()
            }
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Meeting {} not found", meeting_id),
            }),
        )
            .into_response(),
    }
}

/// GET /meetings/:meeting_id/transcript
/// Get transcript for a meeting (accumulated so far)
pub async fn get_meeting_transcript(
    State(state): State<AppState>,
    Path(meeting_id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.sessions.read().await;

    match sessions.get(&meeting_id) {
        Some(session) => {
            let transcript: Vec<TranscriptSegment> = session.get_transcript().await;
            (StatusCode::OK, Json(transcript)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Meeting {} not found", meeting_id),
            }),
        )
            .into_response(),
    }
}

/// GET /health
/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
