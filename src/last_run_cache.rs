use serde::{Deserialize, Serialize};

pub const LAST_RUN_CACHE_PATH: &str = ".last_run_cache.dl";

#[derive(Debug, Serialize, Deserialize)]
pub struct LastRunCache {
    pub url: Vec<String>,
}
