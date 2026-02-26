use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::db::models::SensorReading;

/// In-memory store of the most recent `SensorReading` per device.
///
/// Wrapped in `Arc` so it can be cheaply cloned and shared across tasks.
/// Uses a `tokio::sync::RwLock` so many readers never block each other.
#[derive(Clone, Default)]
pub struct ReadingCache {
    inner: Arc<RwLock<HashMap<String, SensorReading>>>,
}

impl ReadingCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrite the cached reading for `reading.device_id`.
    pub async fn update(&self, reading: SensorReading) {
        self.inner
            .write()
            .await
            .insert(reading.device_id.clone(), reading);
    }

    /// Return a snapshot of all latest readings, one per device.
    pub async fn all(&self) -> Vec<SensorReading> {
        self.inner.read().await.values().cloned().collect()
    }

    /// Return the latest reading for a specific device, if present.
    #[allow(dead_code)]
    pub async fn get(&self, device_id: &str) -> Option<SensorReading> {
        self.inner.read().await.get(device_id).cloned()
    }
}
