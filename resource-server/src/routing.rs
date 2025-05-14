use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ResourceDescriptor {
    pub group: String,
    pub namespace: String,
    pub kind: String,
    pub name: Option<String>,
    pub subresource: Option<String>,
}

impl ResourceDescriptor {
    pub fn parse<S: AsRef<str>>(path: S) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();

        // Remove /api or /api/
        let path = path
            .strip_prefix("/api/")
            .or_else(|| path.strip_prefix("/api"))
            .unwrap_or(path);

        let segments: Vec<&str> = path
            .split('/')
            .filter(|s| !s.trim().is_empty())
            .collect();

        if segments.len() < 3 {
            return Err("Path must have at least 3 segments: group, namespace, kind.".into());
        }

        let group = segments[0].to_string();
        let namespace = segments[1].to_string();
        let kind = segments[2].to_string();
        let name = segments.get(3).map(|s| s.to_string());

        let subresource = if segments.len() > 4 {
            Some(segments[4..].join("/"))
        } else {
            None
        };

        Ok(Self {
            group,
            namespace,
            kind,
            name,
            subresource,
        })
    }
}
