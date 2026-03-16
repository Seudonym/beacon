use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};

#[derive(Debug, Clone)]
pub struct AppState {
    pub rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}
