use std::{fs::{self, File}, io::{Read, Write}, path::{Path, PathBuf}, vec};

use async_trait::async_trait;
use tokio::sync::Mutex;
use walkdir::WalkDir;

use super::{StoreContainer, StoreKey, StoreOperation, StoreResult, StoreValue, TransactionalKeyValueStore};

pub struct FileSystemStore {
    root: PathBuf,
    lock: Mutex<()>, // crude global lock
}

impl FileSystemStore {
    pub fn new<P: AsRef<Path>>(root: P) -> StoreResult<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root: root.as_ref().to_path_buf(),
            lock: Mutex::new(()),
        })
    }

    fn container_path(&self, container: &str) -> PathBuf {
        self.root.join(container)
    }

    fn key_path(&self, container: &str, key: &str) -> PathBuf {
        self.container_path(container).join(key)
    }

    fn fs_path_to_key(&self, path: &Path) -> StoreKey {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_path_buf()
            .to_string_lossy()
            .replace("\\", "/")
    }
}

#[async_trait]
impl TransactionalKeyValueStore for FileSystemStore {
    async fn new_container(&self, container: &str) -> StoreResult<()> {
        let path = self.container_path(&container);
        if path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("Container '{}' already exists", container),
            )));
        }
        fs::create_dir_all(path)?;
        Ok(())
    }

    async fn delete_container(&self, container: &str) -> StoreResult<()> {
        let path = self.container_path(&container);
        if !path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Container '{}' does not exist", container),
            )));
        }
        fs::remove_dir_all(path)?;
        Ok(())
    }

    async fn container_exists(&self, container: &str) -> StoreResult<bool> {
        Ok(self.container_path(&container).exists())
    }

    async fn list_containers(&self) -> StoreResult<Vec<StoreContainer>> {
        let mut containers = vec![];
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.is_dir() {
                containers.push(path.to_string_lossy().to_string());
            }
        }

        Ok(containers)
    }

    async fn list_keys(&self, container: &str, key_prefix: Option<&str>) -> StoreResult<Vec<StoreKey>> {
        let container_root: PathBuf = self.container_path(container);
        let container_prefix = format!("{}/", container);

        // Check if the container exists
        if !container_root.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Container '{}' does not exist", container),
            )));
        }
        if !container_root.exists() {
            return Ok(vec![]);
        }
    
        let mut values = Vec::new();
    
        for entry in WalkDir::new(&container_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
    
            if path.is_dir() {
                // get files in the directory
                for file_entry in fs::read_dir(path)? {
                    let file_path = file_entry?.path();

                    if file_path.is_file() {
                        let store_key = self.fs_path_to_key(&file_path);
                        
                        if key_prefix.is_some() && store_key.starts_with(key_prefix.unwrap()) {
                            let cleaned_key = store_key.strip_prefix(&container_prefix).unwrap().to_string();
                            values.push(cleaned_key);
                        } else {
                            let cleaned_key = store_key.strip_prefix(&container_prefix).unwrap().to_string();
                            values.push(cleaned_key);
                        }
                    }
                }
            }
        }
    
        Ok(values)
    }

    async fn get(&self, container: &str, key: &str) -> StoreResult<StoreValue> {
        let path = self.key_path(&container, &key);
        
        if !path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Key '{}' does not exist in container '{}'", key, container),
            )));
        }

        let mut file = File::open(path)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    async fn put(&self, container: &str, key: &str, value: StoreValue) -> StoreResult<StoreValue> {
        let path = self.key_path(&container, &key);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut file = File::create(path)?;
        file.write_all(&value)?;
        Ok(value)
    }

    async fn delete(&self, container: &str, key: &str) -> StoreResult<()> {
        let container_path_buf = self.container_path(&container);
        let container_path = container_path_buf.as_path();
        
        let path = self.key_path(&container, &key);
        if path.exists() {
            fs::remove_file(&path)?;
            let mut current = path.clone();

            loop {
                // Get parent directory
                let parent_path = match current.parent() {
                    Some(parent) => parent.to_path_buf(),
                    None => break, // We've reached the filesystem root or invalid path
                };

                // Break if we've hit the container root or global root
                if parent_path == self.root || parent_path == container_path {
                    break;
                }

                // Only remove if the directory is empty
                if fs::read_dir(&parent_path)?.next().is_none() {
                    fs::remove_dir(&parent_path)?;
                } else {
                    break; // Stop if directory is not empty
                }

                current = parent_path;
            }
        }
        Ok(())
    }

    async fn commit_transaction(&self, ops: Vec<StoreOperation>) -> StoreResult<()> {
        // Crude lock for atomicity
        let _guard = self.lock.lock().await;

        for op in ops {
            match op {
                StoreOperation::Put(container, key, value) => {
                    self.put(&container, &key, value).await?;
                }
                StoreOperation::Delete(container, key) => {
                    self.delete(&container, &key).await?;
                }
            }
        }
        Ok(())
    }

    async fn rename_container(&self, old: &str, new: &str) -> StoreResult<()> {
        let old_path = self.container_path(&old);
        let new_path = self.container_path(&new);
        if !old_path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Container '{}' does not exist", old),
            )));
        }

        if new_path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("Container '{}' already exists", new),
            )));
        }

        fs::rename(old_path, new_path)?;

        Ok(())
    }

    async fn clear_container(&self, container: &str) -> StoreResult<()> {
        let path = self.container_path(container);
        
        if !path.exists() {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Container '{}' does not exist", container),
            )));
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            fs::remove_file(entry.path())?;
        }
        Ok(())
    }
}