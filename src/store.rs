use crate::oauth::{OAuthTokens, client::AccessTokenUpdate};
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
                            refresh_token VARCHAR UNIQUE NOT NULL,
                            expires_at TIMESTAMP NOT NULL,
                            refresh_token_expires_at TIMESTAMP
                        );

                        CREATE TABLE threads (
                            id VARCHAR PRIMARY KEY,
                            snippet VARCHAR NOT NULL,
                            history_id VARCHAR NOT NULL
                        );

                        CREATE TABLE messages (
                            id VARCHAR PRIMARY KEY,
                            thread_id VARCHAR NOT NULL,
                            snippet VARCHAR NOT NULL,
                            history_id VARCHAR NOT NULL,
                            internal_date TIMESTAMP NOT NULL,
                            payload JSON NOT NULL,
                            size_estimate BIGINT NOT NULL,
                            raw VARCHAR NOT NULL,
                            FOREIGN KEY (thread_id) REFERENCES threads (id)
                        );

                        CREATE TYPE message_list_visibility AS ENUM ('show', 'hide');
                        CREATE TYPE label_list_visibility AS ENUM ('labelShow', 'labelShowIfUnread', 'labelHide');
                        CREATE TYPE label_type AS ENUM ('system', 'user');

                        CREATE TABLE labels (
                            id VARCHAR PRIMARY KEY,
                            name VARCHAR NOT NULL,
                            message_list_visibility message_list_visibility NOT NULL,
                            label_list_visibility label_list_visibility NOT NULL,
                            type label_type NOT NULL,
                            messages_total BIGINT NOT NULL,
                            messsages_unread BIGINT NOT NULL,
                            threads_total BIGINT NOT NULL,
                            threads_unread BIGINT NOT NULL,
                            color_text VARCHAR NOT NULL,
                            background_color VARCHAR NOT NULL
                        );

                        CREATE TABLE message_labels (
                            message_id VARCHAR NOT NULL,
                            label_id VARCHAR NOT NULL,
                            FOREIGN KEY (message_id) REFERENCES messages (id),
                            FOREIGN KEY (label_id) REFERENCES labels (id)
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
