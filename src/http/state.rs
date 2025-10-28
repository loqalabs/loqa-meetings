use crate::session::RecordingSession;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state for HTTP handlers
#[derive(Clone)]
pub struct AppState {
    /// Active recording sessions (meeting_id → session)
    pub sessions: Arc<RwLock<HashMap<String, Arc<RecordingSession>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
