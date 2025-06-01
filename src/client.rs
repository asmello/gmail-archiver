use crate::{http::GenericClient, model::UserProfile, oauth::TokenManager};
use reqwest::Url;
use std::sync::LazyLock;

static BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://gmail.googleapis.com/gmail/v1").expect("valid url"));

pub struct GmailClient {
    http_client: GenericClient,
    token_manager: TokenManager,
}

impl GmailClient {
    pub fn new(token_manager: TokenManager) -> Self {
        Self {
            http_client: token_manager.http_client().with_base_url(BASE_URL.clone()),
            token_manager,
        }
    }

    pub async fn profile(&mut self) -> eyre::Result<UserProfile> {
        self.token_manager.update_access_token().await?;
        self.http_client
            .request(["users", "me", "profile"])
            .bearer_auth(self.token_manager.access_token())
            .send()
            .await
    }
}
