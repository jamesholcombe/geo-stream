//! Redis-backed [`StateStore`] for the geo-events engine.
//!
//! # Data model
//!
//! Each entity is stored as a JSON string under the key `{prefix}:entity:{id}`.
//! All keys share a configurable prefix so multiple engine instances can coexist
//! in the same Redis database without colliding.
//!
//! # Connection pooling
//!
//! Uses an `r2d2` connection pool so multiple threads can safely call the store
//! concurrently. Pool size defaults to 5; adjust via [`RedisStateStore::with_pool_size`].
//!
//! # `all_entities`
//!
//! Enumerates all entity keys via `SCAN` then batch-fetches them with `MGET`. This is
//! O(n) over the keyspace — avoid calling it in hot paths. It is used by query methods
//! like `entities_in_zone` / `entities_near_point`, which are inherently read-heavy.

use r2d2::Pool;
use redis::Commands;
use state::{EntityState, StateStore, StoreError};

type RedisPool = Pool<redis::Client>;

/// Redis-backed [`StateStore`]. Stores each entity's JSON-serialized [`EntityState`]
/// under a namespaced key so multiple engine instances can share one Redis database.
pub struct RedisStateStore {
    pool: RedisPool,
    prefix: String,
}

impl RedisStateStore {
    /// Connect to Redis and create a store with the given key prefix.
    ///
    /// `url` accepts any format supported by the `redis` crate: `redis://host:port`,
    /// `rediss://host:port` (TLS), `redis://:password@host:port`, etc.
    ///
    /// `prefix` is prepended to every key: `{prefix}:entity:{id}`.
    pub fn new(url: &str, prefix: impl Into<String>) -> Result<Self, StoreError> {
        Self::with_pool_size(url, prefix, 5)
    }

    /// Like [`new`](Self::new) but with an explicit connection pool size.
    pub fn with_pool_size(
        url: &str,
        prefix: impl Into<String>,
        pool_size: u32,
    ) -> Result<Self, StoreError> {
        let client = redis::Client::open(url)
            .map_err(|e| StoreError::Backend(format!("redis connect: {e}")))?;
        let pool = r2d2::Pool::builder()
            .max_size(pool_size)
            .build(client)
            .map_err(|e| StoreError::Backend(format!("redis pool: {e}")))?;
        Ok(Self {
            pool,
            prefix: prefix.into(),
        })
    }

    fn entity_key(&self, id: &str) -> String {
        format!("{}:entity:{}", self.prefix, id)
    }

    fn conn(&self) -> Result<r2d2::PooledConnection<redis::Client>, StoreError> {
        self.pool
            .get()
            .map_err(|e| StoreError::Backend(format!("redis pool get: {e}")))
    }
}

impl StateStore for RedisStateStore {
    fn get(&self, id: &str) -> Result<Option<EntityState>, StoreError> {
        let key = self.entity_key(id);
        let mut conn = self.conn()?;
        let raw: Option<String> = conn
            .get(&key)
            .map_err(|e| StoreError::Backend(format!("GET {key}: {e}")))?;
        match raw {
            None => Ok(None),
            Some(json) => {
                let st = serde_json::from_str(&json)
                    .map_err(|e| StoreError::Serialization(format!("deserialize {id}: {e}")))?;
                Ok(Some(st))
            }
        }
    }

    fn set(&mut self, id: &str, state: EntityState) -> Result<(), StoreError> {
        let key = self.entity_key(id);
        let json = serde_json::to_string(&state)
            .map_err(|e| StoreError::Serialization(format!("serialize {id}: {e}")))?;
        let mut conn = self.conn()?;
        conn.set::<_, _, ()>(&key, json)
            .map_err(|e| StoreError::Backend(format!("SET {key}: {e}")))?;
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<(), StoreError> {
        let key = self.entity_key(id);
        let mut conn = self.conn()?;
        conn.del::<_, ()>(&key)
            .map_err(|e| StoreError::Backend(format!("DEL {key}: {e}")))?;
        Ok(())
    }

    fn all_entities(&self) -> Result<Vec<(String, EntityState)>, StoreError> {
        let pattern = format!("{}:entity:*", self.prefix);
        let prefix_len = format!("{}:entity:", self.prefix).len();
        let mut conn = self.conn()?;

        // Scan for all matching keys using the SCAN iterator (handles cursor pagination).
        let keys: Vec<String> = conn
            .scan_match::<&str, String>(&pattern)
            .map_err(|e| StoreError::Backend(format!("SCAN {pattern}: {e}")))?
            .collect();

        if keys.is_empty() {
            return Ok(vec![]);
        }

        // Batch-fetch all values.
        let values: Vec<Option<String>> = conn
            .get(keys.clone())
            .map_err(|e| StoreError::Backend(format!("MGET: {e}")))?;

        let mut result = Vec::with_capacity(keys.len());
        for (key, raw) in keys.into_iter().zip(values) {
            let Some(json) = raw else { continue };
            let entity_id = key[prefix_len..].to_string();
            let st: EntityState = serde_json::from_str(&json)
                .map_err(|e| StoreError::Serialization(format!("deserialize {entity_id}: {e}")))?;
            result.push((entity_id, st));
        }
        Ok(result)
    }
}
