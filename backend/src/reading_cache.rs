use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::db::models::{SensorReading, SensorType};

/// In-memory store of the most recent `SensorReading` per `(device_id, SensorType)`.
///
/// Wrapped in `Arc` so it can be cheaply cloned and shared across tasks.
/// Uses `tokio::sync::RwLock` so concurrent readers never block each other.
#[derive(Clone, Default)]
pub struct ReadingCache {
    inner: Arc<RwLock<HashMap<(String, SensorType), SensorReading>>>,
}

impl ReadingCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrite the cached reading for `(reading.device_id, reading.sensor_type)`.
    pub async fn update(&self, reading: SensorReading) {
        self.inner
            .write()
            .await
            .insert((reading.device_id.clone(), reading.sensor_type), reading);
    }

    /// Return a snapshot of all latest readings across every device and sensor type.
    pub async fn all(&self) -> Vec<SensorReading> {
        self.inner.read().await.values().cloned().collect()
    }

    /// Return all latest readings for a specific device (one per sensor type).
    #[allow(dead_code)]
    pub async fn get_device(&self, device_id: &str) -> Vec<SensorReading> {
        self.inner
            .read()
            .await
            .iter()
            .filter(|((id, _), _)| id == device_id)
            .map(|(_, r)| r.clone())
            .collect()
    }

    /// Return the latest reading for a specific `(device_id, sensor_type)`, if present.
    #[allow(dead_code)]
    pub async fn get(&self, device_id: &str, sensor_type: SensorType) -> Option<SensorReading> {
        self.inner
            .read()
            .await
            .get(&(device_id.to_owned(), sensor_type))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::db::models::{SensorReading, SensorType};

    fn make_reading(device_id: &str, sensor_type: SensorType, value: i64) -> SensorReading {
        SensorReading {
            id: Uuid::new_v4(),
            device_id: device_id.to_owned(),
            sensor_type,
            recorded_at: Utc::now(),
            value,
        }
    }

    #[tokio::test]
    async fn empty_cache_returns_nothing() {
        let cache = ReadingCache::new();
        assert!(cache.all().await.is_empty());
        assert!(cache.get("dev1", SensorType::Temperature).await.is_none());
        assert!(cache.get_device("dev1").await.is_empty());
    }

    #[tokio::test]
    async fn update_and_get_single_reading() {
        let cache = ReadingCache::new();
        let r = make_reading("dev1", SensorType::Temperature, 2145);
        cache.update(r.clone()).await;

        let got = cache.get("dev1", SensorType::Temperature).await.unwrap();
        assert_eq!(got.device_id, "dev1");
        assert_eq!(got.sensor_type, SensorType::Temperature);
        assert_eq!(got.value, 2145);
    }

    #[tokio::test]
    async fn update_overwrites_previous_reading() {
        let cache = ReadingCache::new();
        cache.update(make_reading("dev1", SensorType::Temperature, 2000)).await;
        cache.update(make_reading("dev1", SensorType::Temperature, 2500)).await;

        let got = cache.get("dev1", SensorType::Temperature).await.unwrap();
        assert_eq!(got.value, 2500);
        // all() should still be one entry
        assert_eq!(cache.all().await.len(), 1);
    }

    #[tokio::test]
    async fn different_sensor_types_are_separate_entries() {
        let cache = ReadingCache::new();
        cache.update(make_reading("dev1", SensorType::Temperature, 2145)).await;
        cache.update(make_reading("dev1", SensorType::Humidity, 6050)).await;

        assert_eq!(cache.all().await.len(), 2);
        assert_eq!(cache.get("dev1", SensorType::Temperature).await.unwrap().value, 2145);
        assert_eq!(cache.get("dev1", SensorType::Humidity).await.unwrap().value, 6050);
    }

    #[tokio::test]
    async fn different_devices_are_separate_entries() {
        let cache = ReadingCache::new();
        cache.update(make_reading("dev1", SensorType::Temperature, 2000)).await;
        cache.update(make_reading("dev2", SensorType::Temperature, 3000)).await;

        assert_eq!(cache.all().await.len(), 2);
        assert_eq!(cache.get("dev1", SensorType::Temperature).await.unwrap().value, 2000);
        assert_eq!(cache.get("dev2", SensorType::Temperature).await.unwrap().value, 3000);
    }

    #[tokio::test]
    async fn get_device_returns_only_that_devices_readings() {
        let cache = ReadingCache::new();
        cache.update(make_reading("dev1", SensorType::Temperature, 2145)).await;
        cache.update(make_reading("dev1", SensorType::Humidity, 6050)).await;
        cache.update(make_reading("dev2", SensorType::Temperature, 1800)).await;

        let dev1 = cache.get_device("dev1").await;
        assert_eq!(dev1.len(), 2);
        assert!(dev1.iter().all(|r| r.device_id == "dev1"));

        let dev2 = cache.get_device("dev2").await;
        assert_eq!(dev2.len(), 1);
        assert_eq!(dev2[0].value, 1800);
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let cache = ReadingCache::new();
        let clone = cache.clone();

        cache.update(make_reading("dev1", SensorType::RelayState, 1)).await;

        // Clone sees the same data
        let got = clone.get("dev1", SensorType::RelayState).await.unwrap();
        assert_eq!(got.value, 1);
    }
}
