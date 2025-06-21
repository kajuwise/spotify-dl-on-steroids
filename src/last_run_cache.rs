use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct LastRunCache {
    pub(crate) url: Vec<String>
}