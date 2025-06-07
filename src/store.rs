use crate::{
    model::{AttachmentId, Label, LabelId, Message, MessageId},
    oauth::{OAuthTokens, client::AccessTokenUpdate},
};
use chrono::{DateTime, Utc};
use duckdb::{Connection, OptionalExt, params};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let mut conn = Connection::open(path)?;
        Self::init_or_migrate_db(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_or_migrate_db(conn: &mut Connection) -> eyre::Result<()> {
        if let Ok(version) = conn.query_row("SELECT * FROM version", [], |row| row.get(0)) {
            // migrate
            match version {
                0 => (), // current version
                version => eyre::bail!("unrecognized database version: {version}"),
            }
        } else {
            // init
            conn.execute_batch(
                "
                    CREATE TABLE tokens (
                        access_token VARCHAR NOT NULL,
                        refresh_token VARCHAR PRIMARY KEY,
                        expires_at TIMESTAMP NOT NULL,
                        refresh_token_expires_at TIMESTAMP
                    );

                    CREATE TYPE message_list_visibility AS ENUM ('SHOW', 'HIDE');
                    CREATE TYPE label_list_visibility AS ENUM ('SHOW', 'SHOW_IF_UNREAD', 'HIDE');
                    CREATE TYPE label_type AS ENUM ('SYSTEM', 'USER');

                    CREATE TABLE labels (
                        id VARCHAR PRIMARY KEY,
                        name VARCHAR NOT NULL,
                        message_list_visibility message_list_visibility,
                        label_list_visibility label_list_visibility,
                        type label_type NOT NULL,
                        color_text VARCHAR,
                        background_color VARCHAR
                    );

                    CREATE TABLE messages (
                        id VARCHAR PRIMARY KEY,
                        thread_id VARCHAR NOT NULL,
                        snippet VARCHAR,
                        history_id VARCHAR NOT NULL,
                        internal_date TIMESTAMP NOT NULL,
                        size_estimate BIGINT NOT NULL
                    );

                    CREATE TABLE message_labels (
                        message_id VARCHAR,
                        label_id VARCHAR,
                        PRIMARY KEY (message_id, label_id),
                        FOREIGN KEY (message_id) REFERENCES messages (id),
                        FOREIGN KEY (label_id) REFERENCES labels (id)
                    );

                    CREATE TABLE message_parts (
                        message_id VARCHAR NOT NULL,
                        part_id VARCHAR NOT NULL,
                        mime_type VARCHAR,
                        filename VARCHAR,
                        headers STRUCT(name VARCHAR, value VARCHAR)[],
                        children VARCHAR[],
                        PRIMARY KEY (message_id, part_id),
                        FOREIGN KEY (message_id) REFERENCES messages (id)
                    );

                    CREATE TABLE message_part_body (
                        message_id VARCHAR NOT NULL,
                        part_id VARCHAR NOT NULL,
                        attachment_id VARCHAR,
                        size BIGINT NOT NULL,
                        data BLOB,
                        FOREIGN KEY (message_id, part_id)
                            REFERENCES message_parts(message_id, part_id),
                        FOREIGN KEY (message_id) REFERENCES messages (id)
                    );

                    CREATE TABLE version AS SELECT 0;
                    ",
            )?;
        }

        Ok(())
    }

    pub fn load_tokens(&self) -> eyre::Result<Option<OAuthTokens>> {
        let tokens = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT * FROM tokens", [], |row| {
                Ok(OAuthTokens {
                    access_token: row.get::<_, String>(0)?.into(),
                    refresh_token: row.get::<_, String>(1)?.into(),
                    expires_at: as_datetime(row, 2)?,
                    refresh_token_expires_at: as_datetime_optional(row, 3)?,
                })
            })
            .optional()?;
        Ok(tokens)
    }

    pub fn set_tokens(&self, tokens: &OAuthTokens) -> eyre::Result<()> {
        let OAuthTokens {
            access_token,
            refresh_token,
            expires_at,
            refresh_token_expires_at,
        } = tokens;
        tracing::debug!("token expires: {}", expires_at.to_rfc3339());
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO tokens VALUES (?, ?, ?, ?)",
            params![
                access_token.as_str(),
                refresh_token.as_str(),
                expires_at.to_rfc3339(),
                refresh_token_expires_at.map(|t| t.to_rfc3339())
            ],
        )?;
        Ok(())
    }

    pub fn update_access_token(&self, update: AccessTokenUpdate) -> eyre::Result<()> {
        let AccessTokenUpdate {
            access_token,
            expires_at,
        } = update;
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO tokens (access_token, expires_at) VALUES (?, ?)",
            params![access_token.as_str(), expires_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn contains_label(&self, id: &LabelId) -> eyre::Result<bool> {
        let count: usize = self.conn.lock().unwrap().query_row(
            "SELECT count(*) FROM labels WHERE id = ?",
            [id.as_str()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_label(&self, label: &Label) -> eyre::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO labels VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                label.id.as_str(),
                label.name.as_str(),
                label.message_list_visibility.map(<&str>::from),
                label.label_list_visibility.map(<&str>::from),
                <&str>::from(label.r#type),
                label.color.as_ref().map(|c| c.text_color.as_str()),
                label.color.as_ref().map(|c| c.background_color.as_str()),
            ],
        )?;
        Ok(())
    }

    pub fn contains_message(&self, id: &MessageId) -> eyre::Result<bool> {
        let count: usize = self.conn.lock().unwrap().query_row(
            "SELECT count(*) FROM messages WHERE id = ?",
            [id.as_str()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_message(&self, message: &Message) -> eyre::Result<()> {
        let mut guard = self.conn.lock().unwrap();
        let tr = guard.transaction()?;
        tr.execute(
            "INSERT INTO messages VALUES (?, ?, ?, ?, ?, ?)",
            params![
                message.id.as_str(),
                message.thread_id.as_str(),
                message.snippet.as_str(),
                message.history_id.as_str(),
                message.internal_date.to_rfc3339(),
                message.size_estimate,
            ],
        )?;
        for label_id in &message.label_ids {
            tr.execute(
                "INSERT INTO message_labels VALUES (?, ?)",
                params![message.id.as_str(), label_id.as_str()],
            )?;
        }
        let mut message_parts = Vec::from([&message.payload]);
        while let Some(message_part) = message_parts.pop() {
            let mut children = Vec::new();
            if let Some(parts) = &message_part.parts {
                for part in parts {
                    children.push(part.part_id.to_string());
                    message_parts.push(part);
                }
            }
            // the duckdb crate does not support composite types directly.
            // see https://github.com/duckdb/duckdb-rs/issues/394
            let children = serde_json::to_string(&children)?;
            let headers = serde_json::to_string(&message_part.headers)?;
            tr.execute(
                "INSERT INTO message_parts VALUES (?, ?, ?, ?, ?, ?::JSON::TEXT[])",
                params![
                    message.id.as_str(),
                    message_part.part_id.as_str(),
                    message_part.mime_type.as_str(),
                    message_part.filename.as_str(),
                    headers,
                    children
                ],
            )?;
            tr.execute(
                "INSERT INTO message_part_body VALUES (?, ?, ?, ?, ?)",
                params![
                    message.id.as_str(),
                    message_part.part_id.as_str(),
                    message_part
                        .body
                        .attachment_id
                        .as_ref()
                        .map(AttachmentId::as_str),
                    message_part.body.size,
                    message_part.body.data
                ],
            )?;
        }
        tr.commit()?;
        Ok(())
    }
}

fn as_datetime(row: &duckdb::Row, idx: usize) -> duckdb::Result<DateTime<Utc>> {
    let val = row.get(idx)?;
    DateTime::from_timestamp_millis(val)
        .ok_or_else(|| duckdb::Error::IntegralValueOutOfRange(idx, val.into()))
}

fn as_datetime_optional(row: &duckdb::Row, idx: usize) -> duckdb::Result<Option<DateTime<Utc>>> {
    let Some(val) = row.get::<_, Option<i64>>(idx)? else {
        return Ok(None);
    };
    Ok(Some(DateTime::from_timestamp_millis(val).ok_or_else(
        || duckdb::Error::IntegralValueOutOfRange(idx, val.into()),
    )?))
}
