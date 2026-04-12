//! DocumentDB persistent store backed by Azure Cosmos DB for MongoDB (vCore).
//!
//! Mapping:
//!   container  →  MongoDB collection
//!   key        →  document `_id` (string)
//!   value      →  native BSON document — every JSON field is a real BSON field,
//!                 directly queryable and indexable in MongoDB.
//!
//! Every value written through this store must be valid UTF-8 JSON.  On `put`
//! the bytes are parsed into a `serde_json::Value`, converted to a BSON
//! `Document`, and stored with `_id` set to `key`.  On `get` the BSON document
//! is converted back to a `serde_json::Value` and serialised to JSON bytes,
//! giving a lossless round-trip for all types used by the resource model.
//!
//! MongoDB-backed persistent store for Kuiper resources.

use anyhow::Context;
use async_trait::async_trait;
use mongodb::{
    bson::{self, doc, Document},
    options::FindOptions,
    Client, Collection, Database,
};

use super::{StoreContainer, StoreKey, StoreOperation, StoreResult, StoreValue, TransactionalKeyValueStore};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse JSON bytes into a native BSON document and set `_id` to `key`.
/// Every field in the original JSON becomes a queryable BSON field.
fn make_doc(key: &str, value: &StoreValue) -> anyhow::Result<Document> {
    let json: serde_json::Value =
        serde_json::from_slice(value).context("Store value is not valid JSON")?;
    let mut doc = bson::to_document(&json).context("Failed to convert JSON value to BSON")?;
    doc.insert("_id", key);
    Ok(doc)
}

/// Convert a raw BSON document back to JSON bytes, stripping the `_id` field
/// that was injected by [`make_doc`].
fn doc_to_value(mut doc: Document) -> anyhow::Result<StoreValue> {
    doc.remove("_id");
    let json: serde_json::Value =
        bson::from_document(doc).context("Failed to convert BSON document to JSON")?;
    serde_json::to_vec(&json).context("Failed to serialise JSON value")
}

// ── Public store type ─────────────────────────────────────────────────────────

pub struct DocumentDbStore {
    client: Client,
    db: Database,
}

impl DocumentDbStore {
    /// Connect to an Azure Cosmos DB for MongoDB (vCore) instance using the
    /// provided connection string and target database name.  A `ping` is issued
    /// on construction to fail-fast on bad credentials or network issues.
    pub async fn new(connection_string: &str, database: &str) -> StoreResult<Self> {
        let options = mongodb::options::ClientOptions::parse(connection_string)
            .await
            .context("Failed to parse DocumentDB connection string")?;

        let client = Client::with_options(options)
            .context("Failed to create DocumentDB client")?;

        // Verify connectivity before handing the store to callers.
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .context("DocumentDB ping failed — check connection string and network")?;

        tracing::info!("DocumentDB connection verified (database: {})", database);

        Ok(Self {
            db: client.database(database),
            client,
        })
    }

    fn collection(&self, container: &str) -> Collection<Document> {
        self.db.collection(container)
    }
}

// ── Helper: escape a string for use as a MongoDB regex pattern ────────────────

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '^' | '$' | '|' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

// ── TransactionalKeyValueStore impl ──────────────────────────────────────────

#[async_trait]
impl TransactionalKeyValueStore for DocumentDbStore {
    // ── Container operations ──────────────────────────────────────────────────

    async fn container_exists(&self, container: &str) -> StoreResult<bool> {
        let names = self
            .db
            .list_collection_names()
            .await
            .context("Failed to list MongoDB collections")?;
        Ok(names.iter().any(|n| n == container))
    }

    async fn new_container(&self, container: &str) -> StoreResult<()> {
        if self.container_exists(container).await? {
            return Err(anyhow::anyhow!("Container '{}' already exists", container));
        }
        // MongoDB creates collections lazily; create an explicit collection to
        // satisfy the "already-exists" semantics expected by callers.
        self.db
            .create_collection(container)
            .await
            .with_context(|| format!("Failed to create collection '{}'", container))?;
        Ok(())
    }

    async fn delete_container(&self, container: &str) -> StoreResult<()> {
        if !self.container_exists(container).await? {
            return Err(anyhow::anyhow!("Container '{}' does not exist", container));
        }
        self.collection(container)
            .drop()
            .await
            .with_context(|| format!("Failed to drop collection '{}'", container))?;
        Ok(())
    }

    async fn list_containers(&self) -> StoreResult<Vec<StoreContainer>> {
        self.db
            .list_collection_names()
            .await
            .context("Failed to list MongoDB collections")
    }

    async fn rename_container(&self, old: &str, new: &str) -> StoreResult<()> {
        if !self.container_exists(old).await? {
            return Err(anyhow::anyhow!("Container '{}' does not exist", old));
        }
        if self.container_exists(new).await? {
            return Err(anyhow::anyhow!("Container '{}' already exists", new));
        }

        // MongoDB has no native rename for collections on a different database.
        // Copy all documents then drop the source.
        let src = self.collection(old);
        let dst = self.collection(new);

        let mut cursor = src
            .find(doc! {})
            .await
            .with_context(|| format!("Failed to open cursor on '{}'", old))?;

        let mut batch: Vec<Document> = Vec::new();

        while cursor.advance().await.context("Cursor advance failed")? {
            let doc = cursor
                .deserialize_current()
                .context("Failed to deserialize document")?;
            batch.push(doc);
        }

        if !batch.is_empty() {
            dst.insert_many(batch)
                .await
                .with_context(|| format!("Failed to copy documents to '{}'", new))?;
        }

        src.drop()
            .await
            .with_context(|| format!("Failed to drop source collection '{}'", old))?;

        Ok(())
    }

    async fn clear_container(&self, container: &str) -> StoreResult<()> {
        if !self.container_exists(container).await? {
            return Err(anyhow::anyhow!("Container '{}' does not exist", container));
        }
        self.collection(container)
            .delete_many(doc! {})
            .await
            .with_context(|| format!("Failed to clear collection '{}'", container))?;
        Ok(())
    }

    // ── Key / value operations ────────────────────────────────────────────────

    async fn list_keys(
        &self,
        container: &str,
        key_prefix: Option<&str>,
    ) -> StoreResult<Vec<StoreKey>> {
        if !self.container_exists(container).await? {
            return Err(anyhow::anyhow!("Container '{}' does not exist", container));
        }

        let filter = match key_prefix {
            Some(prefix) => {
                let escaped = regex_escape(prefix);
                doc! { "_id": { "$regex": format!("^{}", escaped) } }
            }
            None => doc! {},
        };

        // Project only `_id` to avoid fetching field data.
        let opts = FindOptions::builder()
            .projection(doc! { "_id": 1 })
            .build();

        let mut cursor = self.collection(container)
            .find(filter)
            .with_options(opts)
            .await
            .with_context(|| format!("Failed to list keys in '{}'", container))?;

        let mut keys = Vec::new();
        while cursor.advance().await.context("Cursor advance failed")? {
            let doc = cursor
                .deserialize_current()
                .context("Failed to deserialize key document")?;
            if let Some(bson::Bson::String(id)) = doc.get("_id") {
                keys.push(id.clone());
            }
        }

        Ok(keys)
    }

    async fn get(&self, container: &str, key: &str) -> StoreResult<StoreValue> {
        let doc = self
            .collection(container)
            .find_one(doc! { "_id": key })
            .await
            .with_context(|| format!("Failed to get '{}' from '{}'", key, container))?
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in container '{}'", key, container))?;

        doc_to_value(doc)
    }

    async fn put(&self, container: &str, key: &str, value: StoreValue) -> StoreResult<StoreValue> {
        let doc = make_doc(key, &value)?;

        self.collection(container)
            .replace_one(doc! { "_id": key }, doc)
            .upsert(true)
            .await
            .with_context(|| format!("Failed to put '{}' into '{}'", key, container))?;

        Ok(value)
    }

    async fn delete(&self, container: &str, key: &str) -> StoreResult<()> {
        self.collection(container)
            .delete_one(doc! { "_id": key })
            .await
            .with_context(|| format!("Failed to delete '{}' from '{}'", key, container))?;
        Ok(())
    }

    /// Execute multiple put/delete operations in a single MongoDB transaction.
    ///
    /// Requires a replica-set or sharded cluster — Azure Cosmos DB for MongoDB
    /// vCore satisfies this requirement.
    async fn commit_transaction(&self, ops: Vec<StoreOperation>) -> StoreResult<()> {
        let mut session = self
            .client
            .start_session()
            .await
            .context("Failed to start MongoDB session")?;

        session
            .start_transaction()
            .await
            .context("Failed to start MongoDB transaction")?;

        for op in ops {
            match op {
                StoreOperation::Put(container, key, value) => {
                    let doc = make_doc(&key, &value)?;
                    self.collection(&container)
                        .replace_one(doc! { "_id": &key }, doc)
                        .upsert(true)
                        .session(&mut session)
                        .await
                        .with_context(|| {
                            format!("Transaction: failed to put '{}' into '{}'", key, container)
                        })?;
                }
                StoreOperation::Delete(container, key) => {
                    self.collection(&container)
                        .delete_one(doc! { "_id": &key })
                        .session(&mut session)
                        .await
                        .with_context(|| {
                            format!(
                                "Transaction: failed to delete '{}' from '{}'",
                                key, container
                            )
                        })?;
                }
            }
        }

        session
            .commit_transaction()
            .await
            .context("Failed to commit MongoDB transaction")?;

        Ok(())
    }
}
