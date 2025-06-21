use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LastRunCache {
    pub url: Vec<String>,
}
