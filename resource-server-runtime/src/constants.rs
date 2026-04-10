/// The key-value container used to store all `SystemObject` resources.
pub(crate) const RESOURCE_CONTAINER: &str = "resource";

/// The group under which built-in system extension types (e.g. `ResourceDefinition`) live.
pub(crate) const SYSTEM_EXTENSION_GROUP: &str = "ext.api.cloud-api.dev";

/// The version string for all built-in types.
pub(crate) const SYSTEM_API_VERSION: &str = "v1alpha1";

/// The pseudo-namespace used for system-scoped (cluster-wide) resources.
pub(crate) const GLOBAL_NAMESPACE: &str = "global";

/// Constructs the storage key for a resource: `{namespace}/{resource}` (lower-cased).
pub(crate) fn resource_key(namespace: &str, resource: Option<&str>) -> String {
    format!("{}/{}", namespace, resource.unwrap_or("")).to_lowercase()
}
