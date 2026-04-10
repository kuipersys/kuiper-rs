mod core;

pub use core::RESERVED_UID_PREFIX;

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::{
    admission_policy::AdmissionPolicy,
    resource_definition::{ResourceDefinition, ResourceDefinitionVersion},
    service_endpoint::ServiceEndpoint,
};
use anyhow::Context;
use kuiper_runtime::data::TransactionalKeyValueStore;
use tokio::sync::RwLock;

use crate::constants::{
    resource_key, GLOBAL_NAMESPACE, RESOURCE_CONTAINER, SYSTEM_API_VERSION, SYSTEM_EXTENSION_GROUP,
};

/// The resource-path prefix used to list / store all `ResourceDefinition` objects.
fn definition_resource_path(name: &str) -> String {
    format!(
        "{}/{}/ResourceDefinition/{}",
        SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION, name
    )
}

fn definition_list_prefix() -> String {
    resource_key(
        GLOBAL_NAMESPACE,
        Some(&format!(
            "{}/{}/ResourceDefinition/",
            SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION
        )),
    )
}

// ── ResourceRegistry ─────────────────────────────────────────────────────────

pub struct ResourceRegistry {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,

    /// `{group}/{kind}` → `ResourceDefinition`
    resources: HashMap<String, ResourceDefinition>,

    /// `{group}/{kind}/{version}` → `ResourceDefinitionVersion`
    resource_versions: HashMap<String, ResourceDefinitionVersion>,
}

impl ResourceRegistry {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self {
            store,
            resources: HashMap::new(),
            resource_versions: HashMap::new(),
        }
    }

    // ── Public query API ──────────────────────────────────────────────────────

    pub fn get_definition(&self, group: &str, kind: &str) -> Option<&ResourceDefinition> {
        self.resources
            .get(&format!("{}/{}", group, kind).to_lowercase())
    }

    pub fn version_exists(&self, group: &str, kind: &str, version: &str) -> bool {
        self.resource_versions
            .contains_key(&format!("{}/{}/{}", group, kind, version).to_lowercase())
    }

    pub fn get_version(
        &self,
        group: &str,
        kind: &str,
        version: &str,
    ) -> Option<&ResourceDefinitionVersion> {
        self.resource_versions
            .get(&format!("{}/{}/{}", group, kind, version).to_lowercase())
    }

    // ── Extension-type store lookups ──────────────────────────────────────────

    /// Retrieves a `ServiceEndpoint` by name from the store.
    pub async fn get_service_endpoint(&self, name: &str) -> anyhow::Result<ServiceEndpoint> {
        let path = format!(
            "{}/{}/ServiceEndpoint/{}",
            SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION, name
        );
        let key = resource_key(GLOBAL_NAMESPACE, Some(&path));
        let store = self.store.read().await;
        let bytes = store
            .get(RESOURCE_CONTAINER, &key)
            .await
            .context(format!("ServiceEndpoint '{}' not found", name))?;
        serde_json::from_slice(&bytes).context("Failed to deserialize ServiceEndpoint")
    }

    /// Lists all `AdmissionPolicy` objects whose target matches `group`/`kind`.
    pub async fn get_admission_policies(
        &self,
        group: &str,
        kind: &str,
    ) -> anyhow::Result<Vec<AdmissionPolicy>> {
        let prefix = resource_key(
            GLOBAL_NAMESPACE,
            Some(&format!(
                "{}/{}/AdmissionPolicy/",
                SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION
            )),
        );
        let store = self.store.read().await;
        let keys = store
            .list_keys(RESOURCE_CONTAINER, Some(&prefix))
            .await
            .context("Failed to list AdmissionPolicy keys")?;

        let mut matching = Vec::new();
        for key in &keys {
            if let Ok(bytes) = store.get(RESOURCE_CONTAINER, key).await {
                if let Ok(policy) = serde_json::from_slice::<AdmissionPolicy>(&bytes) {
                    if policy.spec.target.group.eq_ignore_ascii_case(group)
                        && policy.spec.target.kind.eq_ignore_ascii_case(kind)
                    {
                        matching.push(policy);
                    }
                }
            }
        }
        Ok(matching)
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Seeds core definitions and loads all persisted definitions.
    /// Call once after building the runtime.
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let core_defs = core::core_resource_definitions();

        for def in core_defs {
            self.persist_if_absent(&def).await?;
            self.index_definition(def);
        }

        self.load_from_store().await
    }

    /// Re-reads all persisted definitions from the store.
    /// Called after every successful `set` of a `ResourceDefinition`.
    pub async fn reload(&mut self) -> anyhow::Result<()> {
        self.resources.clear();
        self.resource_versions.clear();
        self.load_from_store().await
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Writes `def` to the store only if the key does not already exist.
    async fn persist_if_absent(&self, def: &ResourceDefinition) -> anyhow::Result<()> {
        let store = self.store.write().await;

        if !store
            .container_exists(RESOURCE_CONTAINER)
            .await
            .context("Failed to check resource container")?
        {
            store
                .new_container(RESOURCE_CONTAINER)
                .await
                .context("Failed to create resource container")?;
        }

        let key = resource_key(
            GLOBAL_NAMESPACE,
            Some(&definition_resource_path(&def.metadata.name)),
        );

        if store.get(RESOURCE_CONTAINER, &key).await.is_ok() {
            return Ok(());
        }

        // Stamp identity fields before first write.
        let mut def = def.clone();
        if def.metadata.uid.is_nil() {
            def.metadata.uid = uuid::Uuid::new_v4();
        }
        if def.metadata.creation_timestamp.is_none() {
            def.metadata.creation_timestamp = Some(chrono::Utc::now().timestamp_micros());
        }
        if def.metadata.resource_version.is_none() {
            def.metadata.resource_version = Some(uuid::Uuid::new_v4().to_string());
        }

        let bytes =
            serde_json::to_vec_pretty(&def).context("Failed to serialize ResourceDefinition")?;

        store
            .put(RESOURCE_CONTAINER, &key, bytes)
            .await
            .context("Failed to persist core ResourceDefinition")?;

        Ok(())
    }

    /// Scans the store for all keys under the `ResourceDefinition` prefix and
    /// indexes every parsed definition.
    async fn load_from_store(&mut self) -> anyhow::Result<()> {
        let prefix = definition_list_prefix();

        let raw_entries: Vec<Vec<u8>> = {
            let store = self.store.read().await;

            let keys = store
                .list_keys(RESOURCE_CONTAINER, Some(&prefix))
                .await
                .context("Failed to list ResourceDefinition keys")?;

            let mut entries = Vec::with_capacity(keys.len());
            for key in &keys {
                if let Ok(bytes) = store.get(RESOURCE_CONTAINER, key).await {
                    entries.push(bytes);
                }
            }
            entries
        };

        for bytes in raw_entries {
            if let Ok(def) = serde_json::from_slice::<ResourceDefinition>(&bytes) {
                self.index_definition(def);
            }
        }

        Ok(())
    }

    fn index_definition(&mut self, def: ResourceDefinition) {
        for (k, v) in def.enabled_versions() {
            self.resource_versions.insert(k, v);
        }
        self.resources.insert(def.registry_key(), def);
    }
}
