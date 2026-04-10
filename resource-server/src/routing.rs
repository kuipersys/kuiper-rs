// ResourceDescriptor lives in resource-server-api and is re-exported here
// so that existing code inside this crate can continue to use `routing::ResourceDescriptor`.
pub use resource_server_api::routes::ResourceDescriptor;
