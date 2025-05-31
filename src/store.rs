use crate::oauth::OAuthTokens;
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
        Self::init_db(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_db(conn: &mut Connection) -> eyre::Result<()> {
        conn.execute_batch(
            "
                CREATE TABLE IF NOT EXISTS tokens (
                    access_token VARCHAR NOT NULL,
                    refresh_token VARCHAR UNIQUE NOT NULL,
                    expires_at TIMESTAMP NOT NULL,
                    refresh_token_expires_at TIMESTAMP
                );
            ",
        )?;
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
