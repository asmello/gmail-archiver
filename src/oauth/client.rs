use super::{AuthzCode, ClientCredentials, CodeVerifier, OAuthTokens};
use crate::{
    http::GenericClient,
    oauth::{AccessToken, State, TOKEN_ENDPOINT, server},
};
use chrono::{DateTime, Utc};
use reqwest::{Method, Url};
use serde::Deserialize;
use std::time::Duration;

const AUTHZ_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const REDIRECT_URI: &str = "http://127.0.0.1:47218/callback";

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ErrorResponse {
    error: String,
    error_description: String,
}

type HttpClient = GenericClient<ErrorResponse>;

pub struct PartialOAuthClient {
    creds: ClientCredentials,
    http_client: HttpClient,
    verifier: CodeVerifier,
}

impl PartialOAuthClient {
    pub fn new(creds: ClientCredentials, verifier: CodeVerifier) -> Self {
        Self {
            http_client: GenericClient::builder(TOKEN_ENDPOINT.clone()).build(),
            creds,
            verifier,
        }
    }

    pub async fn exchange_code_for_tokens(&self, code: AuthzCode) -> eyre::Result<OAuthClient> {
        #[derive(Deserialize)]
        struct TokensResponse {
            access_token: String,
            expires_in: u64,
            refresh_token: String,
            refresh_token_expires_in: Option<u64>,
            // scope: String,
        }

        let resp = self
            .http_client
            .request::<TokensResponse, _>([])
            .method(Method::POST)
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

        let now = Utc::now();
        let expires_at = now + Duration::from_secs(resp.expires_in);
        let refresh_token_expires_at = resp
            .refresh_token_expires_in
            .map(|v| now + Duration::from_secs(v));
        tracing::debug!(
            "access token expires in {} seconds: {}",
            resp.expires_in,
            expires_at
        );
        Ok(OAuthClient {
            creds: self.creds.clone(),
            http_client: self.http_client.clone(),
            tokens: OAuthTokens {
                access_token: resp.access_token.into(),
                refresh_token: resp.refresh_token.into(),
                expires_at,
                refresh_token_expires_at,
            },
        })
    }
}

pub struct OAuthClient {
    creds: ClientCredentials,
    tokens: OAuthTokens,
    http_client: HttpClient,
}

impl OAuthClient {
    pub fn new(creds: ClientCredentials, tokens: OAuthTokens) -> Self {
        Self {
            creds,
            tokens,
            http_client: GenericClient::builder(TOKEN_ENDPOINT.clone()).build(),
        }
    }

    pub async fn authorize(creds: ClientCredentials) -> eyre::Result<Self> {
        let code_verifier = CodeVerifier::new();
        let state = State::new();

        let mut url = Url::parse(AUTHZ_ENDPOINT)?;
        url.query_pairs_mut()
            .append_pair("client_id", creds.id.as_str())
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("response_type", "code")
            .append_pair("scope", "https://mail.google.com/")
            .append_pair("code_challenge", &code_verifier.to_s256())
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state.as_str());

        tracing::debug!("opening browser window");
        webbrowser::open(url.as_str())?;
        println!("Authorization URL: {url}");
        server::wait_response(creds, state, code_verifier).await
    }

    pub fn http_client<E>(&self) -> GenericClient<E> {
        self.http_client.clone().coerce_error()
    }

    pub fn tokens(&self) -> &OAuthTokens {
        &self.tokens
    }

    pub fn access_token(&self) -> &AccessToken {
        &self.tokens.access_token
    }

    pub async fn check_access_token(&mut self) -> eyre::Result<Option<AccessTokenUpdate>> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: AccessToken,
            expires_in: u64,
            // scope: String,
        }

        if self.tokens.expires_at > Utc::now() {
            tracing::debug!(expires_at = %self.tokens.expires_at, "access token still valid");
            return Ok(None);
        }
        tracing::info!("access token expired, fetching new one");

        let TokenResponse {
            access_token,
            expires_in,
        } = self
            .http_client
            .request([])
            .method(Method::POST)
            .form(&[
                ("client_id", self.creds.id.as_str()),
                ("client_secret", self.creds.secret.as_str()),
                ("refresh_token", self.tokens.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        let expires_at = Utc::now() + Duration::from_secs(expires_in);
        tracing::info!("new token expires at {expires_at}");

        self.tokens.access_token = access_token.clone();
        self.tokens.expires_at = expires_at;

        Ok(Some(AccessTokenUpdate {
            access_token,
            expires_at,
        }))
    }
}

pub struct AccessTokenUpdate {
    pub access_token: AccessToken,
    pub expires_at: DateTime<Utc>,
}
