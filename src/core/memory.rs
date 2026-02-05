//! Conversation memory storage using SQLite
//!
//! Provides persistent storage for conversation history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

use crate::conversation::{Message, Role};

/// A stored message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl From<StoredMessage> for Message {
    fn from(stored: StoredMessage) -> Self {
        let role = match stored.role.as_str() {
            "system" => Role::System,
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => Role::User,
        };
        Message {
            role,
            content: stored.content,
        }
    }
}

/// Memory store for conversation persistence
pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    /// Create a new memory store with the given SQLite database path
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        // Create parent directories if they don't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    /// Create an in-memory store for testing
    pub fn new_in_memory() -> Self {
        // Create a pool synchronously for in-memory databases
        // In real usage, prefer async initialization
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        Self { pool }
    }

    /// Create an in-memory store asynchronously
    pub async fn new_in_memory_async() -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    /// Initialize the database schema
    async fn init_schema(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_messages_conversation
            ON messages(conversation_id, created_at)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Save a message to a conversation
    pub async fn save_message(
        &self,
        conversation_id: &str,
        message: &Message,
    ) -> Result<i64, sqlx::Error> {
        // Ensure conversation exists
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversations (id) VALUES (?)
            "#,
        )
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;

        // Update conversation timestamp
        sqlx::query(
            r#"
            UPDATE conversations SET updated_at = datetime('now') WHERE id = ?
            "#,
        )
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;

        // Insert message
        let role_str = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        let result = sqlx::query(
            r#"
            INSERT INTO messages (conversation_id, role, content)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(conversation_id)
        .bind(role_str)
        .bind(&message.content)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get all messages in a conversation
    pub async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT role, content
            FROM messages
            WHERE conversation_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content)| {
                let role = match role.as_str() {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Message { role, content }
            })
            .collect())
    }

    /// Get recent messages from a conversation (with limit)
    pub async fn get_recent_messages(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT role, content
            FROM messages
            WHERE conversation_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(conversation_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        // Reverse to get chronological order
        Ok(rows
            .into_iter()
            .rev()
            .map(|(role, content)| {
                let role = match role.as_str() {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Message { role, content }
            })
            .collect())
    }

    /// Search messages by content
    pub async fn search_messages(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StoredMessage>, sqlx::Error> {
        let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
            r#"
            SELECT id, conversation_id, role, content, created_at
            FROM messages
            WHERE content LIKE ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(format!("%{}%", query))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, conversation_id, role, content, created_at)| StoredMessage {
                id,
                conversation_id,
                role,
                content,
                created_at: DateTime::parse_from_rfc3339(&format!("{}Z", created_at))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    /// Delete a conversation and all its messages
    pub async fn delete_conversation(&self, conversation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM messages WHERE conversation_id = ?")
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get all conversation IDs
    pub async fn list_conversations(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT id FROM conversations ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store() {
        let store = MemoryStore::new_in_memory_async().await.unwrap();

        let conversation_id = "test-conv-1";

        // Save messages
        store
            .save_message(
                conversation_id,
                &Message {
                    role: Role::User,
                    content: "Hello".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .save_message(
                conversation_id,
                &Message {
                    role: Role::Assistant,
                    content: "Hi there!".to_string(),
                },
            )
            .await
            .unwrap();

        // Retrieve messages
        let messages = store.get_conversation(conversation_id).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_search_messages() {
        let store = MemoryStore::new_in_memory_async().await.unwrap();

        store
            .save_message(
                "conv1",
                &Message {
                    role: Role::User,
                    content: "How do I read a file?".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .save_message(
                "conv2",
                &Message {
                    role: Role::User,
                    content: "What's the weather?".to_string(),
                },
            )
            .await
            .unwrap();

        let results = store.search_messages("file", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("file"));
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let store = MemoryStore::new_in_memory_async().await.unwrap();

        store
            .save_message(
                "conv1",
                &Message {
                    role: Role::User,
                    content: "Message 1".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .save_message(
                "conv2",
                &Message {
                    role: Role::User,
                    content: "Message 2".to_string(),
                },
            )
            .await
            .unwrap();

        let conversations = store.list_conversations().await.unwrap();
        assert_eq!(conversations.len(), 2);
    }
}
