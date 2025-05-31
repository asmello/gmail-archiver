use super::{AuthzCode, ClientCredentials, CodeVerifier, OAuthTokens};
use crate::oauth::{REDIRECT_URI, TOKEN_ENDPOINT};
use chrono::Utc;
use reqwest::Url;
use serde::Deserialize;
use std::time::Duration;

pub struct OAuthClient {
    creds: ClientCredentials,
    verifier: CodeVerifier,
    http_client: reqwest::Client,
}

impl OAuthClient {
    pub fn new(creds: ClientCredentials, verifier: CodeVerifier) -> Self {
        Self {
            creds,
            verifier,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn exchange_code_for_tokens(&self, code: AuthzCode) -> eyre::Result<OAuthTokens> {
        #[derive(Deserialize)]
        struct TokensResponse {
            access_token: String,
            expires_in: u64,
            refresh_token: String,
            refresh_token_expires_in: Option<u64>,
            scope: String,
        }

        #[derive(Deserialize)]
        struct ErrorResponse {
            error: String,
            error_description: String,
        }

        let url = Url::parse(TOKEN_ENDPOINT)?;
        let resp = self
            .http_client
            .post(url)
            .form(&[
                ("code", code.as_str()),
                ("code_verifier", self.verifier.as_str()),
                ("client_id", self.creds.id.as_str()),
                ("client_secret", self.creds.secret.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(_) => {
                    eyre::bail!("request failed with status {status}");
                }
            };
            let text = match String::from_utf8(bytes.into()) {
                Ok(text) => text,
                Err(err) => {
                    eyre::bail!(
                        "request failed with status {status}.\nPayload: {:?}",
                        err.into_bytes()
                    );
                }
            };
            let payload = match serde_json::from_str::<ErrorResponse>(&text) {
                Ok(payload) => payload,
                Err(_) => {
                    eyre::bail!("request failed with status {status}: {text}");
                }
            };
            eyre::bail!(
                "request failed with status {status}\n\nerror: {}\ndescription: {}",
                payload.error,
                payload.error_description
            );
        }

        let resp: TokensResponse = resp.json().await?;

        let now = Utc::now();
        Ok(OAuthTokens {
            access_token: resp.access_token.into(),
            refresh_token: resp.refresh_token.into(),
            expires_at: now + Duration::from_secs(resp.expires_in),
            refresh_token_expires_at: resp
                .refresh_token_expires_in
                .map(|v| now + Duration::from_secs(v)),
        })
    }
}
