use core::fmt;
use std::marker::PhantomData;

use bon::bon;
use eyre::Context;
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;

use crate::oauth::AccessToken;

pub struct GenericClient<E = ()> {
    base_url: Url,
    http_client: reqwest::Client,
    _error: PhantomData<E>,
}

impl<E> Clone for GenericClient<E> {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            http_client: self.http_client.clone(),
            _error: Default::default(),
        }
    }
}

#[bon]
impl<E> GenericClient<E> {
    #[builder]
    pub fn new(
        #[builder(start_fn)] base_url: Url,
        #[builder(default)] http_client: reqwest::Client,
    ) -> Self {
        Self {
            base_url,
            http_client,
            _error: Default::default(),
        }
    }

    pub fn coerce_error<E2>(&self) -> GenericClient<E2> {
        GenericClient {
            base_url: self.base_url.clone(),
            http_client: self.http_client.clone(),
            _error: Default::default(),
        }
    }

    pub fn with_base_url(&self, base_url: Url) -> Self {
        Self {
            base_url,
            http_client: self.http_client.clone(),
            _error: Default::default(),
        }
    }
}

#[bon]
impl<E: DeserializeOwned + fmt::Debug> GenericClient<E> {
    #[builder(finish_fn = send)]
    pub async fn request<T: DeserializeOwned>(
        &self,
        #[builder(start_fn)] path: impl IntoIterator<Item = &str>,
        #[builder(default = Method::GET)] method: Method,
        form: Option<&[(&str, &str)]>,
        bearer_auth: Option<&AccessToken>,
    ) -> eyre::Result<T> {
        let url = {
            let mut url = self.base_url.clone();
            url.path_segments_mut().expect("valid url").extend(path);
            url
        };

        let mut request_builder = self.http_client.request(method, url);
        if let Some(bearer) = bearer_auth {
            request_builder = request_builder.bearer_auth(bearer.as_str());
        }
        if let Some(form) = form {
            request_builder = request_builder.form(form);
        }
        let request = request_builder.build()?;
        tracing::debug!(
            method = %request.method(),
            url = %request.url(),
            headers = ?request.headers(),
            "executing request");
        let response = self.http_client.execute(request).await?;

        let status = response.status();
        if !status.is_success() {
            let bytes = match response.bytes().await {
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
            let payload = match serde_json::from_str::<E>(&text) {
                Ok(payload) => payload,
                Err(_) => {
                    eyre::bail!("request failed with status {status}: {text}");
                }
            };
            eyre::bail!("request failed with status {status}\n\n{payload:?}",);
        }

        let data = response.bytes().await.wrap_err("empty body")?;
        let text = str::from_utf8(&data).wrap_err_with(|| format!("raw body: {data:?}"))?;
        serde_json::from_str(text).wrap_err_with(|| format!("unexpected payload: {text}"))
    }
}
