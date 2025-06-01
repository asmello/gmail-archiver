pub mod client;
mod server;

use crate::{http::GenericClient, store::Store};
use chrono::{DateTime, Utc};
use client::OAuthClient;
use reqwest::Url;
use secrets_file::SecretsFile;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, LazyLock},
};

pub static TOKEN_ENDPOINT: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://oauth2.googleapis.com/token").expect("valid url"));

pub struct TokenManager {
    client: OAuthClient,
    store: Store,
}

impl TokenManager {
    pub fn new(client: OAuthClient, store: Store) -> Self {
        Self { client, store }
    }

    pub fn http_client<E>(&self) -> GenericClient<E> {
        self.client.http_client()
    }

    pub async fn update_access_token(&mut self) -> eyre::Result<()> {
        if let Some(update) = self.client.check_access_token().await? {
            tracing::debug!("access token refreshed, will update database");
            self.store.update_access_token(update)?;
        }
        Ok(())
    }

    pub fn access_token(&self) -> &AccessToken {
        self.client.access_token()
    }
}

macro_rules! impl_as_str {
    ($($ty:ty),+) => {
        $(
            impl $ty {
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }
        )+
    };
}
impl_as_str!(
    ClientId,
    ClientSecret,
    AuthzCode,
    AccessToken,
    RefreshToken,
    State
);
pub(crate) use impl_as_str;

macro_rules! impl_from_string {
    ($($ty:ty),+) => {
        $(
            impl From<String> for $ty {
                fn from(value: String) -> Self {
                    Self(value.into())
                }
            }
        )+
    };
}
impl_from_string!(AccessToken, RefreshToken);

#[derive(Clone, Deserialize)]
pub struct ClientId(String);

#[derive(Clone, Deserialize)]
pub struct ClientSecret(String);

#[derive(Clone)]
pub struct ClientCredentials {
    pub id: ClientId,
    pub secret: ClientSecret,
}

impl ClientCredentials {
    pub fn load_from_file(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let secrets = serde_json::from_reader::<_, SecretsFile>(reader)?.into_inner();
        Ok(Self {
            id: secrets.client_id,
            secret: secrets.client_secret,
        })
    }
}

mod secrets_file {
    use super::{ClientId, ClientSecret};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SecretsFile {
        Installed(ApplicationSecrets),
        Web(ApplicationSecrets),
    }

    impl SecretsFile {
        pub fn into_inner(self) -> ApplicationSecrets {
            match self {
                SecretsFile::Installed(inner) => inner,
                SecretsFile::Web(inner) => inner,
            }
        }
    }

    #[derive(Deserialize)]
    pub struct ApplicationSecrets {
        pub client_id: ClientId,
        pub client_secret: ClientSecret,
    }
}

pub struct CodeVerifier([u8; 128]);

impl CodeVerifier {
    const VALID_CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

    pub fn new() -> Self {
        use rand::seq::IndexedRandom;
        let mut rng = rand::rng();
        Self(std::array::from_fn(|_| {
            Self::VALID_CHARS.choose(&mut rng).copied().unwrap()
        }))
    }

    pub fn to_s256(&self) -> String {
        use base64::prelude::*;

        let hashed = Sha256::digest(self.0);
        BASE64_URL_SAFE_NO_PAD.encode(hashed)
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: created from ascii characters
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}

#[derive(Deserialize, PartialEq, Eq)]
pub struct State(String);

impl State {
    fn new() -> Self {
        use rand::{Rng, distr::Alphanumeric};
        Self(
            rand::rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect(),
        )
    }
}

#[derive(Deserialize)]
pub struct AuthzCode(String);

#[derive(Debug, Deserialize, Clone)]
pub struct AccessToken(Arc<str>);

#[derive(Debug, Deserialize)]
pub struct RefreshToken(String);

#[derive(Debug)]
pub struct OAuthTokens {
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub expires_at: DateTime<Utc>,
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
}
