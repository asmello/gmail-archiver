use crate::{
    http::GenericClient,
    model::{
        Label, LabelId, LabelList, Message, MessageId, MinimalMessage, MinimalThread, PageToken,
    },
    oauth::{AccessToken, TokenManager},
};
use reqwest::Url;
use serde::Deserialize;
use std::sync::{Arc, LazyLock};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::{Stream, wrappers::ReceiverStream};

static BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://gmail.googleapis.com/gmail/v1").expect("valid url"));

#[derive(Clone)]
pub struct GmailClient {
    inner: Arc<GmailClientInner>,
}

struct GmailClientInner {
    http_client: GenericClient,
    token_manager: Mutex<TokenManager>,
}

impl GmailClient {
    pub fn new(token_manager: TokenManager) -> Self {
        Self {
            inner: Arc::new(GmailClientInner {
                http_client: token_manager.http_client().with_base_url(BASE_URL.clone()),
                token_manager: Mutex::new(token_manager),
            }),
        }
    }

    async fn access_token(&self) -> eyre::Result<AccessToken> {
        let mut guard = self.inner.token_manager.lock().await;
        guard.update_access_token().await?;
        Ok(guard.access_token().clone())
    }

    pub async fn label(&self, id: &LabelId) -> eyre::Result<Label> {
        self.inner
            .http_client
            .request(["users", "me", "labels", id.as_str()])
            .access_token(self.access_token().await?)
            .send()
            .await
    }

    pub async fn list_labels(&self) -> eyre::Result<LabelList> {
        self.inner
            .http_client
            .request(["users", "me", "labels"])
            .access_token(self.access_token().await?)
            .send()
            .await
    }

    pub fn list_threads(&self) -> impl Stream<Item = eyre::Result<MinimalThread>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct ThreadsPage {
            threads: Vec<MinimalThread>,
            next_page_token: Option<PageToken>,
        }

        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(self.clone().result_wrapper(tx, |this, tx| async move {
            let fetch_page = async |page_token: Option<PageToken>| -> eyre::Result<ThreadsPage> {
                this.inner
                    .http_client
                    .request(["users", "me", "threads"])
                    .access_token(this.access_token().await?)
                    .maybe_query(
                        page_token
                            .as_ref()
                            .map(|t| [("pageToken", t.as_str())])
                            .as_ref()
                            .map(|t| t.as_slice()),
                    )
                    .send()
                    .await
            };

            let mut page = fetch_page(None).await?;
            for thread in page.threads {
                if tx.send(Ok(thread)).await.is_err() {
                    return Ok(());
                }
            }
            while let Some(token) = page.next_page_token {
                page = fetch_page(Some(token)).await?;
                for thread in page.threads {
                    if tx.send(Ok(thread)).await.is_err() {
                        return Ok(());
                    }
                }
            }
            Ok(())
        }));
        ReceiverStream::new(rx)
    }

    pub async fn message(&self, id: &MessageId) -> eyre::Result<Message> {
        self.inner
            .http_client
            .request(["users", "me", "messages", id.as_str()])
            .access_token(self.access_token().await?)
            .query(&[("format", "full")])
            .send()
            .await
    }

    pub fn list_messages(&self) -> impl Stream<Item = eyre::Result<MinimalMessage>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct MessagesPage {
            messages: Vec<MinimalMessage>,
            next_page_token: Option<PageToken>,
        }

        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(self.clone().result_wrapper(tx, |this, tx| async move {
            let fetch_page = async |page_token: Option<PageToken>| -> eyre::Result<MessagesPage> {
                this.inner
                    .http_client
                    .request(["users", "me", "messages"])
                    .access_token(this.access_token().await?)
                    .maybe_query(
                        page_token
                            .as_ref()
                            .map(|t| [("pageToken", t.as_str())])
                            .as_ref()
                            .map(|t| t.as_slice()),
                    )
                    .send()
                    .await
            };

            let mut page = fetch_page(None).await?;
            for msg in page.messages {
                if tx.send(Ok(msg)).await.is_err() {
                    return Ok(());
                }
            }
            while let Some(token) = page.next_page_token {
                page = fetch_page(Some(token)).await?;
                for msg in page.messages {
                    if tx.send(Ok(msg)).await.is_err() {
                        return Ok(());
                    }
                }
            }
            Ok(())
        }));
        ReceiverStream::new(rx)
    }

    async fn result_wrapper<T, F>(
        self,
        tx: mpsc::Sender<eyre::Result<T>>,
        maker: impl FnOnce(Self, mpsc::Sender<eyre::Result<T>>) -> F,
    ) where
        F: Future<Output = eyre::Result<()>>,
    {
        if let Err(err) = maker(self, tx.clone()).await {
            let _ = tx.send(Err(err)).await;
        }
    }
}
