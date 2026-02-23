use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- DEFINITIONS ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMetadata {
    pub id: Uuid,
    pub path: String, // Vault relative path
    pub size: u64,
    pub checksum: String, // SHA-256
    pub last_modified: DateTime<Utc>,
    pub version: u64, // Incremented at every update
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Client {
    pub id: Uuid,
    pub name: String,
    pub public_key: Vec<u8>,
    pub registered_at: DateTime<Utc>,
}

// WebSocket protocol messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    FileUpdated { metadata: FileMetadata },
    FileDeleted { file_id: Uuid },
    ConflictDetected { file_id: Uuid, clients: Vec<Uuid> },
    Ping,
    Pong,
}

// --- IMPLEMENTATIONS ---

impl FileMetadata {
    pub fn new(path: String, size: u64, checksum: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            path,
            size,
            checksum,
            last_modified: Utc::now(),
            version: 1,
        }
    }

    pub fn update(&mut self, size: u64, checksum: String) {
        self.size = size;
        self.checksum = checksum;
        self.last_modified = Utc::now();
        self.version = self.version.saturating_add(1);
    }
}

impl Client {
    pub fn new(name: String, public_key: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            public_key,
            registered_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_metadata_creation() {
        let path = "test.txt".to_string();
        let size = 100;
        let checksum = "hash".to_string();
        let metadata = FileMetadata::new(path.clone(), size, checksum.clone());

        assert_eq!(metadata.path, path);
        assert_eq!(metadata.size, size);
        assert_eq!(metadata.checksum, checksum);
        assert_eq!(metadata.version, 1);
    }

    #[test]
    fn test_file_metadata_update() {
        let mut metadata = FileMetadata::new("test.txt".to_string(), 100, "hash1".to_string());
        let old_time = metadata.last_modified;
        let old_version = metadata.version;

        metadata.update(200, "hash2".to_string());

        assert_eq!(metadata.size, 200);
        assert_eq!(metadata.checksum, "hash2");
        assert_eq!(metadata.version, old_version + 1);
        assert!(metadata.last_modified >= old_time);
    }

    #[test]
    fn test_file_metadata_update_saturates_version() {
        let mut metadata = FileMetadata::new("test.txt".to_string(), 100, "hash1".to_string());
        metadata.version = u64::MAX;

        metadata.update(200, "hash2".to_string());

        assert_eq!(metadata.version, u64::MAX);
    }

    #[test]
    fn test_client_creation() {
        let name = "Alice".to_string();
        let public_key = vec![1, 2, 3];
        let client = Client::new(name.clone(), public_key.clone());

        assert_eq!(client.name, name);
        assert_eq!(client.public_key, public_key);
    }

    #[test]
    fn test_ws_message_serialization() {
        let metadata = FileMetadata::new("test.txt".to_string(), 100, "hash".to_string());
        let msg = WsMessage::FileUpdated {
            metadata: metadata.clone(),
        };

        let json = serde_json::to_string(&msg).expect("Failed to serialize");
        assert!(json.contains("\"type\":\"file_updated\""));

        let decoded: WsMessage = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_ws_message_ping_pong() {
        let ping = WsMessage::Ping;
        let json = serde_json::to_string(&ping).unwrap();
        assert_eq!(json, "{\"type\":\"ping\"}");

        let pong: WsMessage = serde_json::from_str("{\"type\":\"pong\"}").unwrap();
        assert!(matches!(pong, WsMessage::Pong));
    }
}
