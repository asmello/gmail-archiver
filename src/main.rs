mod client;
mod http;
mod macros;
mod model;
mod oauth;
mod store;

use clap::Parser;
use client::GmailClient;
use oauth::{ClientCredentials, TokenManager, client::OAuthClient};
use std::path::PathBuf;
use store::Store;
use tokio_stream::StreamExt;

#[derive(Parser)]
struct Args {
    secrets_file: PathBuf,
    #[arg(long, default_value = "data.db")]
    db: PathBuf,
}

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();
    setup_logging();

    let store = Store::open(args.db)?;
    let creds = ClientCredentials::load_from_file(args.secrets_file)?;
    let oauth_client = match store.load_tokens()? {
        Some(tokens) => {
            tracing::info!("tokens loaded from database");
            OAuthClient::new(creds, tokens)
        }
        None => {
            tracing::info!("no tokens in database, initiating authorization flow");
            let oauth_client = OAuthClient::authorize(creds).await?;
            tracing::info!("authorization flow successful");
            store.set_tokens(oauth_client.tokens())?;
            oauth_client
        }
    };
    let token_manager = TokenManager::new(oauth_client, store.clone());
    let client = GmailClient::new(token_manager);
    fetch_everything(&client, &store).await?;

    Ok(())
}

async fn fetch_everything(client: &GmailClient, store: &Store) -> eyre::Result<()> {
    fetch_labels(client, store).await?;
    fetch_messages(client, store).await?;
    Ok(())
}

async fn fetch_labels(client: &GmailClient, store: &Store) -> eyre::Result<()> {
    let labels = client.list_labels().await?;
    tracing::info!("processing {} labels", labels.labels.len());
    for label in labels.labels {
        if store.contains_label(&label.id)? {
            tracing::debug!(id = %label.id, "label already stored");
            continue;
        }
        tracing::debug!(id = %label.id, "fetching label from remote");
        let label = client.label(&label.id).await?;
        store.insert_label(&label)?;
        tracing::debug!(id = %label.id, "label stored successfully");
    }
    Ok(())
}

async fn fetch_messages(client: &GmailClient, store: &Store) -> eyre::Result<()> {
    let mut messages = client.list_messages();
    while let Some(message) = messages.next().await.transpose()? {
        if store.contains_message(&message.id)? {
            tracing::debug!(id = %message.id, "message already stored");
            continue;
        }
        let message = client.message(&message.id).await?;
        store.insert_message(&message)?;
        tracing::debug!(id = %message.id, "message stored successfully");
    }
    Ok(())
}
