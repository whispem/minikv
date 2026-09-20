use crate::common::Result;
use crate::coordinator::metadata::{KeyMetadata, MetadataStore};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    PutKey(KeyMetadata),
    DeleteKey(String),
}

impl Command {
    pub fn encode(&self) -> Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| crate::Error::Internal(format!("cannot encode command: {}", e)))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes)
            .map_err(|e| crate::Error::Internal(format!("cannot decode command: {}", e)))
    }
}

pub fn apply(store: &MetadataStore, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let result = match Command::decode(data) {
        Ok(Command::PutKey(meta)) => store.put_key(&meta),
        Ok(Command::DeleteKey(key)) => store.delete_key(&key),
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        tracing::error!("Cannot apply raft entry to the metadata store: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::metadata::KeyState;
    use tempfile::tempdir;

    #[test]
    fn commands_update_the_metadata_store() {
        let dir = tempdir().unwrap();
        let store = MetadataStore::open(dir.path().join("meta")).unwrap();
        let meta = KeyMetadata {
            key: "photos/cat.png".to_string(),
            blob_id: "photos/cat.png#1".to_string(),
            replicas: vec!["vol-1".to_string(), "vol-2".to_string()],
            size: 3,
            blake3: "abc".to_string(),
            created_at: 1,
            updated_at: 1,
            state: KeyState::Active,
        };
        apply(&store, &Command::PutKey(meta).encode().unwrap());
        let stored = store.get_key("photos/cat.png").unwrap().unwrap();
        assert_eq!(stored.replicas.len(), 2);

        apply(&store, &[]);
        assert!(store.get_key("photos/cat.png").unwrap().is_some());

        apply(
            &store,
            &Command::DeleteKey("photos/cat.png".to_string())
                .encode()
                .unwrap(),
        );
        assert!(store.get_key("photos/cat.png").unwrap().is_none());
    }
}
