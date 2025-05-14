use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;

use super::{StoreContainer, StoreKey, StoreOperation, StoreResult, StoreValue, TransactionalKeyValueStore};

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
    async fn list(
        &self,
        container: &StoreContainer,
        key_prefix: &StoreKey,
    ) -> StoreResult<Option<Vec<StoreValue>>> {
        let data = self.data.lock().unwrap();
        let container_data = data.get(container);

        if let Some(map) = container_data {
            let result = map
                .iter()
                .filter(|(k, _)| k.starts_with(key_prefix))
                .map(|(_, v)| v.clone())
                .collect::<Vec<_>>();
            return Ok(Some(result));
        }

        Ok(None)
    }

    async fn get(&self, container: &StoreContainer, key: &StoreKey) -> StoreResult<Option<StoreValue>> {
        let data = self.data.lock().unwrap();
        Ok(data.get(container).and_then(|m| m.get(key)).cloned())
    }

    async fn put(&self, container: &StoreContainer, key: StoreKey, value: StoreValue) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        data.entry(container.clone()).or_default().insert(key, value);
        Ok(())
    }

    async fn delete(&self, container: &StoreContainer, key: &StoreKey) -> StoreResult<()> {
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

    async fn new_container(&self, container: StoreContainer) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if data.contains_key(&container) {
            return Err(format!("Container '{}' already exists", container).into());
        }
        data.insert(container, HashMap::new());
        Ok(())
    }

    async fn delete_container(&self, container: &StoreContainer) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if data.remove(container).is_none() {
            return Err(format!("Container '{}' does not exist", container).into());
        }
        Ok(())
    }

    async fn container_exists(&self, container: &StoreContainer) -> StoreResult<bool> {
        let data = self.data.lock().unwrap();
        Ok(data.contains_key(container))
    }

    async fn list_containers(&self) -> StoreResult<Vec<StoreContainer>> {
        let data = self.data.lock().unwrap();
        Ok(data.keys().cloned().collect())
    }

    async fn rename_container(
        &self,
        old: StoreContainer,
        new: StoreContainer,
    ) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if !data.contains_key(old) {
            return Err(format!("Container '{}' does not exist", old).into());
        }
        if data.contains_key(&new) {
            return Err(format!("Target container '{}' already exists", new).into());
        }

        let content = data.remove(old).unwrap();
        data.insert(new, content);
        Ok(())
    }

    async fn clear_container(&self, container: &StoreContainer) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        if let Some(container_map) = data.get_mut(container) {
            container_map.clear();
            Ok(())
        } else {
            Err(format!("Container '{}' does not exist", container).into())
        }
    }
}
