mod client;
mod http;
mod macros;
mod model;
mod oauth;
mod store;

use clap::Parser;
use client::GmailClient;
use model::{AttachmentId, FullMessage, MessageId};
use oauth::{ClientCredentials, TokenManager, client::OAuthClient};
use std::path::PathBuf;
use store::Store;
use tokio_stream::StreamExt;
use tracing::Level;

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
    let profile = client.profile().await?;
    let total = profile.messages_total;
    let stored = store.message_count()?;
    tracing::info!("total messages: {total}, stored: {stored}");
    let mut fetched = 0;
    let mut messages = client.list_messages();
    while let Some(message) = messages.next().await.transpose()? {
        if store.contains_message(&message.id)? {
            tracing::debug!(id = %message.id, "message already stored");
            for attachment_id in store.attachment_ids(&message.id)? {
                fetch_attachment(client, store, &message.id, &attachment_id).await?;
            }
        } else {
            let message = client.full_message(&message.id).await?;
            store.insert_message(&message)?;
            tracing::debug!(id = %message.id, "message stored successfully");
            for attachment_id in extract_attachment_ids(&message) {
                fetch_attachment(client, store, &message.id, attachment_id).await?;
            }
        }
        if store.contains_raw_message(&message.id)? {
            tracing::debug!(id = %message.id, "raw message already stored");
        } else {
            let raw_message = client.raw_message(&message.id).await?;
            store.insert_raw_message(&message.id, &raw_message.raw)?;
            tracing::debug!(id = %message.id, "raw message stored successfully");
        }
        fetched += 1;
        if fetched % 1000 == 0 {
            tracing::info!(total, "fetched {}K messages", fetched / 1000);
        }
    }
    Ok(())
}

#[tracing::instrument(level = Level::DEBUG, skip_all, fields(msg_id = %message_id, id = %attachment_id))]
async fn fetch_attachment(
    client: &GmailClient,
    store: &Store,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> eyre::Result<()> {
    if store.contains_message_attachment(message_id, attachment_id)? {
        tracing::debug!("attachment already stored");
    } else {
        let attachment = client.attachment(message_id, attachment_id).await?;
        store.insert_attachment(message_id, attachment_id, &attachment)?;
        tracing::debug!("attachment stored succesfully");
    }
    Ok(())
}

fn extract_attachment_ids(message: &FullMessage) -> Vec<&AttachmentId> {
    let mut attachments = Vec::new();
    let mut parts = Vec::from([&message.payload]);
    while let Some(part) = parts.pop() {
        if let Some(attachment_id) = &part.body.attachment_id {
            attachments.push(attachment_id);
        }
        parts.extend(part.parts.iter());
    }
    attachments
}
