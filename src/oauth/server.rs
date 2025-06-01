use super::{AuthzCode, ClientCredentials, CodeVerifier, State as OAuthState, client::OAuthClient};
use axum::{
    Router,
    extract::{Query, State},
    response::Html,
    routing::get,
};
use error::ServerError;
use eyre::eyre;
use maud::html;
use serde::Deserialize;
use state::ServerState;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::{net::TcpListener, sync::oneshot};
use tokio_util::sync::CancellationToken;

const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 47218);

pub async fn wait_response(
    creds: ClientCredentials,
    state: OAuthState,
    verifier: CodeVerifier,
) -> eyre::Result<OAuthClient> {
    let token = CancellationToken::new();
    let (tx, rx) = oneshot::channel();
    let router = make_router(ServerState::new(creds, state, verifier, token.clone(), tx));
    let listener = TcpListener::bind(BIND_ADDR).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_handler(token))
        .await?;
    Ok(rx.await?)
}

fn make_router(state: ServerState) -> Router<()> {
    Router::new()
        .route("/callback", get(callback))
        .with_state(state)
}

// when this future completes, shutdown starts
async fn shutdown_handler(token: CancellationToken) {
    token.cancelled().await
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AuthorizationResponse {
    Success(AuthorizationSuccess),
    Error(AuthorizationError),
}

#[derive(Deserialize)]
struct AuthorizationSuccess {
    state: OAuthState,
    code: AuthzCode,
}

#[derive(Deserialize)]
struct AuthorizationError {
    error: String,
}

async fn callback(
    State(server_state): State<ServerState>,
    Query(params): Query<AuthorizationResponse>,
) -> Result<Html<String>, ServerError> {
    match params {
        AuthorizationResponse::Success(AuthorizationSuccess { state, code }) => {
            if state != *server_state.state() {
                return Err(eyre!("invalid state").into());
            }

            let tokens = server_state.client().exchange_code_for_tokens(code).await?;
            server_state.complete(tokens);

            Ok(Html(
                html! {
                    (maud::DOCTYPE)
                    meta charset="utf-8";
                    title { "Gmail Archiver - Authz" }
                    body {
                        h1 {"Success"}
                        p {"You may close this window now."}
                    }
                }
                .into_string(),
            ))
        }
        AuthorizationResponse::Error(error) => Err(ServerError::Authz(error.error)),
    }
}

mod state {
    use crate::oauth::{
        ClientCredentials, CodeVerifier, State,
        client::{OAuthClient, PartialOAuthClient},
    };
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    #[derive(Clone)]
    pub struct ServerState {
        inner: Arc<ServerStateInner>,
    }

    struct ServerStateInner {
        state: State,
        client: PartialOAuthClient,
        cancel: CancellationToken,
        tx: Mutex<Option<oneshot::Sender<OAuthClient>>>,
    }

    impl ServerState {
        pub fn new(
            creds: ClientCredentials,
            state: State,
            verifier: CodeVerifier,
            cancel: CancellationToken,
            tx: oneshot::Sender<OAuthClient>,
        ) -> Self {
            Self {
                inner: Arc::new(ServerStateInner {
                    state,
                    cancel,
                    client: PartialOAuthClient::new(creds, verifier),
                    tx: Mutex::new(Some(tx)),
                }),
            }
        }

        pub fn state(&self) -> &State {
            &self.inner.state
        }

        pub fn client(&self) -> &PartialOAuthClient {
            &self.inner.client
        }

        pub fn complete(self, client: OAuthClient) {
            self.inner.cancel.cancel();
            let maybe_sender = self.inner.tx.lock().unwrap().take();
            if let Some(sender) = maybe_sender {
                let _ = sender.send(client);
            }
        }
    }
}

mod error {
    use axum::{
        http::StatusCode,
        response::{IntoResponse, Response},
    };

    pub enum ServerError {
        Authz(String),
        Generic(eyre::Report),
    }

    impl From<eyre::Report> for ServerError {
        fn from(value: eyre::Report) -> Self {
            Self::Generic(value)
        }
    }

    impl IntoResponse for ServerError {
        fn into_response(self) -> Response {
            match self {
                ServerError::Authz(msg) => (StatusCode::UNAUTHORIZED, msg).into_response(),
                ServerError::Generic(report) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("{report:?}")).into_response()
                }
            }
        }
    }
}
