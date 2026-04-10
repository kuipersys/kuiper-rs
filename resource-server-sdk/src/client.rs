use anyhow::Context;
use kuiper_types::model::resource::SystemObject;

/// Async HTTP client for the resource-server REST API.
///
/// Routes follow the pattern: `/api/{group}/{namespace}/{kind}[/{name}]`
pub struct ResourceServerClient {
    base_url: String,
    client: reqwest::Client,
}

impl ResourceServerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    fn resource_url(&self, group: &str, namespace: &str, kind: &str, name: &str) -> String {
        format!(
            "{}/api/{}/{}/{}/{}",
            self.base_url, group, namespace, kind, name
        )
    }

    fn list_url(&self, group: &str, namespace: &str, kind: &str) -> String {
        format!("{}/api/{}/{}/{}", self.base_url, group, namespace, kind)
    }

    /// Fetches a single resource by name.
    pub async fn get(
        &self,
        group: &str,
        namespace: &str,
        kind: &str,
        name: &str,
    ) -> anyhow::Result<SystemObject> {
        let url = self.resource_url(group, namespace, kind, name);
        self.client
            .get(&url)
            .send()
            .await
            .context("GET request failed")?
            .error_for_status()
            .context("GET returned non-2xx")?
            .json::<SystemObject>()
            .await
            .context("Failed to parse GET response")
    }

    /// Lists all resources of a given kind in a namespace.
    pub async fn list(
        &self,
        group: &str,
        namespace: &str,
        kind: &str,
    ) -> anyhow::Result<Vec<SystemObject>> {
        let url = self.list_url(group, namespace, kind);
        self.client
            .get(&url)
            .send()
            .await
            .context("LIST request failed")?
            .error_for_status()
            .context("LIST returned non-2xx")?
            .json::<Vec<SystemObject>>()
            .await
            .context("Failed to parse LIST response")
    }

    /// Creates or updates a resource (PUT).
    pub async fn set(
        &self,
        group: &str,
        namespace: &str,
        kind: &str,
        name: &str,
        body: &SystemObject,
    ) -> anyhow::Result<SystemObject> {
        let url = self.resource_url(group, namespace, kind, name);
        self.client
            .put(&url)
            .json(body)
            .send()
            .await
            .context("PUT request failed")?
            .error_for_status()
            .context("PUT returned non-2xx")?
            .json::<SystemObject>()
            .await
            .context("Failed to parse PUT response")
    }

    /// Soft-deletes a resource. Returns the updated object if the server
    /// echoes it back (when finalizers are present), or `None` on a 204.
    pub async fn delete(
        &self,
        group: &str,
        namespace: &str,
        kind: &str,
        name: &str,
    ) -> anyhow::Result<Option<SystemObject>> {
        let url = self.resource_url(group, namespace, kind, name);
        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .context("DELETE request failed")?
            .error_for_status()
            .context("DELETE returned non-2xx")?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let obj = resp
            .json::<SystemObject>()
            .await
            .context("Failed to parse DELETE response")?;
        Ok(Some(obj))
    }
}
