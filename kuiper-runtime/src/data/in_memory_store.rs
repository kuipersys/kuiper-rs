use std::{collections::HashMap, sync::Mutex};

use anyhow::anyhow;
use async_trait::async_trait;

use super::{
    StoreContainer, StoreKey, StoreOperation, StoreResult, StoreValue, TransactionalKeyValueStore,
};

pub struct InMemoryStore {
    data: Mutex<HashMap<StoreContainer, HashMap<StoreKey, StoreValue>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl TransactionalKeyValueStore for InMemoryStore {
    async fn list_keys(
        &self,
        container: &str,
        key_prefix: Option<&str>,
    ) -> StoreResult<Vec<StoreKey>> {
        let data = self.data.lock().unwrap();
        let container_data = data.get(container);

        if let Some(map) = container_data {
            let result = match key_prefix {
                Some(prefix) => map
                    .keys()
                    .filter(|k| k.starts_with(prefix))
                    .cloned()
                    .collect::<Vec<_>>(),
                None => map.keys().cloned().collect::<Vec<_>>(),
            };
            return Ok(result);
        }

        Ok(Vec::new())
    }

    async fn get(&self, container: &str, key: &str) -> StoreResult<StoreValue> {
        let data = self.data.lock().unwrap();
        data.get(container)
            .and_then(|m| m.get(key))
            .cloned()
            .ok_or_else(|| anyhow!("Key '{}' not found in container '{}'", key, container))
    }

    async fn put(&self, container: &str, key: &str, value: StoreValue) -> StoreResult<StoreValue> {
        let mut data = self.data.lock().unwrap();
        let container_map = data.entry(container.to_string()).or_default();
        Ok(container_map
            .insert(key.to_string(), value)
            .unwrap_or_default())
    }

    async fn delete(&self, container: &str, key: &str) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if let Some(container_map) = data.get_mut(container) {
            container_map.remove(key);
        }
        Ok(())
    }

    async fn commit_transaction(&self, ops: Vec<StoreOperation>) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        for op in ops {
            match op {
                StoreOperation::Put(container, key, value) => {
                    data.entry(container).or_default().insert(key, value);
                }
                StoreOperation::Delete(container, key) => {
                    if let Some(container_map) = data.get_mut(&container) {
                        container_map.remove(&key);
                    }
                }
            }
        }
        Ok(())
    }

    async fn new_container(&self, container: &str) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if data.contains_key(container) {
            return Err(anyhow!("Container '{}' already exists", container));
        }
        data.insert(container.to_string(), HashMap::new());
        Ok(())
    }

    async fn delete_container(&self, container: &str) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if data.remove(container).is_none() {
            return Err(anyhow!("Container '{}' does not exist", container));
        }
        Ok(())
    }

    async fn container_exists(&self, container: &str) -> StoreResult<bool> {
        let data = self.data.lock().unwrap();
        Ok(data.contains_key(container))
    }

    async fn list_containers(&self) -> StoreResult<Vec<StoreContainer>> {
        let data = self.data.lock().unwrap();
        Ok(data.keys().cloned().collect())
    }

    async fn rename_container(&self, old: &str, new: &str) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if !data.contains_key(old) {
            return Err(anyhow!("Container '{}' does not exist", old));
        }
        if data.contains_key(new) {
            return Err(anyhow!("Target container '{}' already exists", new));
        }

        let content = data.remove(old).unwrap();
        data.insert(new.to_string(), content);
        Ok(())
    }

    async fn clear_container(&self, container: &str) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if let Some(container_map) = data.get_mut(container) {
            container_map.clear();
            Ok(())
        } else {
            Err(anyhow!("Container '{}' does not exist", container))
        }
    }
}
