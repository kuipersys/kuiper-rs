// pub mod in_memory_store;
pub mod file_system_store;

use async_trait::async_trait;

pub type StoreContainer = String;
pub type StoreKey = String;
pub type StoreValue = Vec<u8>; // or `serde_json::Value` if you want it JSON-aware
pub type StoreResult<T> = anyhow::Result<T>;

pub enum StoreOperation {
    Put(StoreContainer, StoreKey, StoreValue),
    Delete(StoreContainer, StoreKey),
}

#[async_trait]
pub trait TransactionalKeyValueStore: Send + Sync {
    async fn list_keys(&self, container: &str, key_prefix: Option<&str>) -> StoreResult<Vec<StoreKey>>;
    async fn get(&self, container: &str, key: &str) -> StoreResult<StoreValue>;
    async fn put(&self, container: &str, key: &str, value: StoreValue) -> StoreResult<StoreValue>;
    async fn delete(&self, container: &str, key: &str) -> StoreResult<()>;
    async fn commit_transaction(&self, ops: Vec<StoreOperation>) -> StoreResult<()>;
    async fn new_container(&self, container: &str) -> StoreResult<()>;
    async fn delete_container(&self, container: &str) -> StoreResult<()>;
    async fn container_exists(&self, container: &str) -> StoreResult<bool>;
    async fn list_containers(&self) -> StoreResult<Vec<StoreContainer>>;
    async fn rename_container(
        &self,
        old: &str,
        new: &str,
    ) -> StoreResult<()>;

    async fn clear_container(&self, container: &str) -> StoreResult<()>;
}

pub struct Transaction<'a> {
    store: &'a dyn TransactionalKeyValueStore,
    staged_ops: Vec<StoreOperation>,
    committed: bool,
}

impl<'a> Transaction<'a> {
    pub fn new(store: &'a dyn TransactionalKeyValueStore) -> Self {
        Self {
            store,
            staged_ops: vec![],
            committed: false,
        }
    }

    pub fn put(&mut self, container: StoreContainer, key: StoreKey, value: StoreValue) {
        self.staged_ops.push(StoreOperation::Put(container, key, value));
    }

    pub fn delete(&mut self, container: StoreContainer, key: StoreKey) {
        self.staged_ops.push(StoreOperation::Delete(container, key));
    }

    pub async fn commit(mut self) -> StoreResult<()> {
        if self.committed || self.staged_ops.is_empty() {
            return Ok(());
        }
    
        self.store.commit_transaction(std::mem::take(&mut self.staged_ops)).await?;
        self.committed = true;
        Ok(())
    }

    pub fn rollback(mut self) {
        self.staged_ops.clear();
        self.committed = true;
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if !self.committed {
            // Log warning or rollback side-effects
            eprintln!("⚠️ Transaction dropped without commit – changes rolled back.");
        }
    }
}