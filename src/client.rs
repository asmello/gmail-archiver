use crate::{
    http::GenericClient,
    model::{FullMessage, MessageId, MinimalMessage, PageToken},
    oauth::{AccessToken, TokenManager},
};
use futures_buffered::FuturesOrdered;
use futures_core::Stream;
use reqwest::Url;
use serde::Deserialize;
use std::{
    ops::ControlFlow,
    sync::{Arc, LazyLock},
};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

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

    // pub async fn profile(&self) -> eyre::Result<UserProfile> {
    //     self.update_token().await?;
    //     self.inner
    //         .http_client
    //         .request(["users", "me", "profile"])
    //         .bearer_auth(self.inner.token_manager.read().await.access_token())
    //         .send()
    //         .await
    // }

    async fn access_token(&self) -> eyre::Result<AccessToken> {
        let mut guard = self.inner.token_manager.lock().await;
        guard.update_access_token().await?;
        Ok(guard.access_token().clone())
    }

    pub fn messages(&self) -> impl Stream<Item = eyre::Result<FullMessage>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct MessagesPage {
            pub messages: Vec<MinimalMessage>,
            pub next_page_token: Option<PageToken>,
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
            let fetch_full_msgs =
                async |msgs: Vec<MinimalMessage>| -> eyre::Result<ControlFlow<()>> {
                    let mut futures: FuturesOrdered<_> = msgs
                        .into_iter()
                        .map(|msg| {
                            let this = this.clone();
                            async move { this.message(&msg.id).await }
                        })
                        .collect();
                    while let Some(msg) = futures.next().await.transpose()? {
                        if tx.send(Ok(msg)).await.is_err() {
                            return Ok(ControlFlow::Break(()));
                        };
                    }
                    Ok(ControlFlow::Continue(()))
                };

            let mut page = fetch_page(None).await?;
            if fetch_full_msgs(page.messages).await?.is_break() {
                return Ok(());
            };
            while let Some(token) = page.next_page_token {
                page = fetch_page(Some(token)).await?;
                if fetch_full_msgs(page.messages).await?.is_break() {
                    return Ok(());
                };
            }
            Ok(())
        }));
        ReceiverStream::new(rx)
    }

    pub async fn message(&self, id: &MessageId) -> eyre::Result<FullMessage> {
        self.inner
            .http_client
            .request(["users", "me", "messages", id.as_str()])
            .access_token(self.access_token().await?)
            .query(&[("format", "full")])
            .send()
            .await
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

    // async fn make_request<T: DeserializeOwned>(&mut self, request: Request) -> eyre::Result<T> {
    //     self.token_manager.update_access_token().await?;
    //     self.http_client.make_request(request).await
    // }
}

// mod paginator {
//     use crate::model::{Page, PageParts, PageToken};

//     use super::GmailClient;
//     use futures_core::Stream;
//     use pin_project::pin_project;
//     use rand::seq::IndexedRandom;
//     use reqwest::{Request, Response};
//     use serde::de::DeserializeOwned;
//     use std::{
//         pin::Pin,
//         task::{Context, Poll},
//     };

//     #[pin_project]
//     pub struct Paginator<'client, P, T> {
//         client: &'client mut GmailClient,
//         iter: std::vec::IntoIter<T>,
//         next_page_token: Option<PageToken>,
//         #[pin]
//         inflight_request: Option<Pin<Box<dyn Future<Output = eyre::Result<P>> + 'client>>>,
//         request_factory: Box<dyn Fn(PageToken) -> Request>,
//     }

//     impl<'client, T: Unpin, P> Stream for Paginator<'client, P, T>
//     where
//         P: Page<T> + Unpin + DeserializeOwned,
//     {
//         type Item = eyre::Result<T>;

//         fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//             let mut this = self.project();
//             while let Some(inflight) = this.inflight_request.as_mut().as_pin_mut() {
//                 match inflight.poll(cx) {
//                     Poll::Ready(Ok(page)) => {
//                         this.inflight_request.take();
//                         let PageParts {
//                             next_page_token,
//                             items,
//                         } = page.decompose();
//                         let mut iter = items.into_iter();
//                         if let Some(first) = iter.next() {
//                             *this.next_page_token = next_page_token;
//                             *this.iter = iter;
//                             return Poll::Ready(Some(Ok(first)));
//                         } else if let Some(page_token) = next_page_token {
//                             let request = (this.request_factory)(page_token);
//                             let fut = this.client.make_request(request);
//                             *this.inflight_request = Some(Box::pin(fut));
//                             continue;
//                         }
//                     }
//                     Poll::Ready(Err(err)) => {
//                         return Poll::Ready(Some(Err(err)));
//                     }
//                     Poll::Pending => return Poll::Pending,
//                 }
//             }
//             if let Some(next) = this.iter.next() {
//                 return Poll::Ready(Some(Ok(next)));
//             }
//             let Some(page_token) = this.next_page_token.take() else {
//                 return Poll::Ready(None);
//             };
//             Poll::Pending
//         }

//         // fn next(&mut self) -> Option<Self::Item> {
//         //     if let Some(next) = self.iter.next() {
//         //         return Some(next);
//         //     }
//         //     let page_token = self.next_page_token.take()?;
//         //     let request = (self.request_factory)(page_token); self.client.make_request(request).await

//         //     todo!()
//         // }
//     }
// }
