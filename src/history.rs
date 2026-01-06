use anyhow::Result;
use librespot::core::SpotifyUri;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
struct StoredHistory {
    playlists: HashMap<String, BTreeSet<String>>,
}

pub struct PlaylistHistory {
    path: PathBuf,
    data: StoredHistory,
}

impl PlaylistHistory {
    pub fn load(path: PathBuf) -> Self {
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default();

        PlaylistHistory { path, data }
    }

    pub fn record_download(&mut self, playlist: &SpotifyUri, track: &SpotifyUri) -> Result<()> {
        if let (Some(playlist_id), Some(track_id)) = (to_uri_string(playlist), to_uri_string(track))
        {
            match self.data.playlists.entry(playlist_id) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(track_id);
                }
                Entry::Vacant(entry) => {
                    let mut set = BTreeSet::new();
                    set.insert(track_id);
                    entry.insert(set);
                }
            }
            self.persist()?;
        }

        Ok(())
    }

    pub fn has_downloaded(&self, playlist: &SpotifyUri, track: &SpotifyUri) -> bool {
        let (Some(playlist_id), Some(track_id)) =
            (to_uri_string(playlist), to_uri_string(track))
        else {
            return false;
        };

        self.data
            .playlists
            .get(&playlist_id)
            .map_or(false, |tracks| tracks.contains(&track_id))
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, serialized)?;
        Ok(())
    }
}

fn to_uri_string(uri: &SpotifyUri) -> Option<String> {
    uri.to_uri().ok()
}
