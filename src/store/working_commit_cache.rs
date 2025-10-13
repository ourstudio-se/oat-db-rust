use crate::model::{Id, WorkingCommit};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache entry for a working commit
#[derive(Clone, Debug)]
struct CacheEntry {
    working_commit: WorkingCommit,
    last_accessed: Instant,
    dirty: bool, // true if modified but not yet persisted
}

/// In-memory cache for working commits with TTL
#[derive(Debug)]
pub struct WorkingCommitCache {
    /// Cache entries keyed by working commit ID
    entries: Arc<RwLock<HashMap<Id, CacheEntry>>>,
    /// Cache entries keyed by (database_id, branch_name) for active working commit lookups
    active_by_branch: Arc<RwLock<HashMap<(Id, String), Id>>>,
    /// Time-to-live for cache entries (1 hour)
    ttl: Duration,
}

impl WorkingCommitCache {
    /// Create a new cache with 1-hour TTL
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            active_by_branch: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Get a working commit from cache if present and not expired
    pub async fn get(&self, id: &Id) -> Option<WorkingCommit> {
        let mut entries = self.entries.write().await;

        if let Some(entry) = entries.get_mut(id) {
            // Check if entry has expired
            if entry.last_accessed.elapsed() > self.ttl {
                entries.remove(id);
                return None;
            }

            // Update access time
            entry.last_accessed = Instant::now();
            Some(entry.working_commit.clone())
        } else {
            None
        }
    }

    /// Get the active working commit ID for a branch from cache
    pub async fn get_active_for_branch(&self, database_id: &Id, branch_name: &str) -> Option<Id> {
        let active_by_branch = self.active_by_branch.read().await;
        let key = (database_id.clone(), branch_name.to_string());
        active_by_branch.get(&key).cloned()
    }

    /// Put a working commit into cache
    pub async fn put(&self, working_commit: WorkingCommit) {
        let mut entries = self.entries.write().await;
        let id = working_commit.id.clone();

        entries.insert(id.clone(), CacheEntry {
            working_commit: working_commit.clone(),
            last_accessed: Instant::now(),
            dirty: false,
        });

        // Update active branch mapping if this is an active working commit
        if working_commit.status == crate::model::WorkingCommitStatus::Active {
            if let Some(ref branch_name) = working_commit.branch_name {
                let mut active_by_branch = self.active_by_branch.write().await;
                let key = (working_commit.database_id.clone(), branch_name.clone());
                active_by_branch.insert(key, id);
            }
        }
    }

    /// Mark a working commit as dirty (modified but not yet persisted)
    /// This allows us to batch writes to Postgres
    pub async fn mark_dirty(&self, id: &Id) {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.dirty = true;
            entry.last_accessed = Instant::now();
        }
    }

    /// Update a working commit in cache and mark it as dirty
    pub async fn update(&self, working_commit: WorkingCommit) {
        let mut entries = self.entries.write().await;
        let id = working_commit.id.clone();

        entries.insert(id, CacheEntry {
            working_commit,
            last_accessed: Instant::now(),
            dirty: true,
        });
    }

    /// Get all dirty entries that need to be persisted to Postgres
    pub async fn get_dirty_entries(&self) -> Vec<WorkingCommit> {
        let entries = self.entries.read().await;
        entries
            .values()
            .filter(|entry| entry.dirty)
            .map(|entry| entry.working_commit.clone())
            .collect()
    }

    /// Mark an entry as clean (persisted to Postgres)
    pub async fn mark_clean(&self, id: &Id) {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.dirty = false;
        }
    }

    /// Remove a working commit from cache
    pub async fn remove(&self, id: &Id) {
        let mut entries = self.entries.write().await;

        // If the entry exists and has branch info, remove from active mapping
        if let Some(entry) = entries.get(id) {
            if let Some(ref branch_name) = entry.working_commit.branch_name {
                let mut active_by_branch = self.active_by_branch.write().await;
                let key = (entry.working_commit.database_id.clone(), branch_name.clone());

                // Only remove if this is the current active working commit for the branch
                if active_by_branch.get(&key) == Some(id) {
                    active_by_branch.remove(&key);
                }
            }
        }

        entries.remove(id);
    }

    /// Clear all expired entries from cache
    pub async fn clear_expired(&self) {
        let mut entries = self.entries.write().await;
        let mut active_by_branch = self.active_by_branch.write().await;

        let now = Instant::now();
        let ttl = self.ttl;

        // Find expired entry IDs
        let expired_ids: Vec<Id> = entries
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_accessed) > ttl)
            .map(|(id, _)| id.clone())
            .collect();

        // Remove expired entries
        for id in &expired_ids {
            if let Some(entry) = entries.get(id) {
                // Remove from active branch mapping
                if let Some(ref branch_name) = entry.working_commit.branch_name {
                    let key = (entry.working_commit.database_id.clone(), branch_name.clone());
                    if active_by_branch.get(&key) == Some(id) {
                        active_by_branch.remove(&key);
                    }
                }
            }
            entries.remove(id);
        }
    }

    /// Clear the entire cache
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        let mut active_by_branch = self.active_by_branch.write().await;
        entries.clear();
        active_by_branch.clear();
    }
}

impl Default for WorkingCommitCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{WorkingCommitStatus, Schema};

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = WorkingCommitCache::new();

        let working_commit = WorkingCommit {
            id: "wc-test-1".to_string(),
            database_id: "db-1".to_string(),
            branch_name: Some("main".to_string()),
            based_on_hash: "abc123".to_string(),
            author: Some("test".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            schema_data: Schema {
                id: "schema-1".to_string(),
                description: None,
                classes: Vec::new(),
            },
            instances_data: Vec::new(),
            status: WorkingCommitStatus::Active,
            merge_state: None,
        };

        // Put into cache
        cache.put(working_commit.clone()).await;

        // Get from cache
        let cached = cache.get(&working_commit.id).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().id, working_commit.id);

        // Get active for branch
        let active_id = cache.get_active_for_branch(&"db-1".to_string(), "main").await;
        assert!(active_id.is_some());
        assert_eq!(active_id.unwrap(), working_commit.id);

        // Remove from cache
        cache.remove(&working_commit.id).await;
        let cached = cache.get(&working_commit.id).await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_dirty_tracking() {
        let cache = WorkingCommitCache::new();

        let working_commit = WorkingCommit {
            id: "wc-test-2".to_string(),
            database_id: "db-1".to_string(),
            branch_name: Some("main".to_string()),
            based_on_hash: "abc123".to_string(),
            author: Some("test".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            schema_data: Schema {
                id: "schema-1".to_string(),
                description: None,
                classes: Vec::new(),
            },
            instances_data: Vec::new(),
            status: WorkingCommitStatus::Active,
            merge_state: None,
        };

        // Put into cache (not dirty initially)
        cache.put(working_commit.clone()).await;
        let dirty = cache.get_dirty_entries().await;
        assert_eq!(dirty.len(), 0);

        // Mark as dirty
        cache.mark_dirty(&working_commit.id).await;
        let dirty = cache.get_dirty_entries().await;
        assert_eq!(dirty.len(), 1);

        // Mark as clean
        cache.mark_clean(&working_commit.id).await;
        let dirty = cache.get_dirty_entries().await;
        assert_eq!(dirty.len(), 0);
    }
}
