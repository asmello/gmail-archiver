use crate::oauth::OAuthTokens;
use duckdb::Connection;
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
        Ok(Self {
            conn: Arc::new(Mutex::new(Connection::open(path)?)),
        })
    }

    pub fn set_tokens(&self, tokens: OAuthTokens) -> eyre::Result<()> {
        Ok(())
    }
}
