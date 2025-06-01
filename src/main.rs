mod client;
mod http;
mod model;
mod oauth;
mod store;

use clap::Parser;
use client::GmailClient;
use oauth::{ClientCredentials, TokenManager, client::OAuthClient};
use std::path::PathBuf;
use store::Store;

#[derive(Parser)]
struct Args {
    secrets_file: PathBuf,
    #[arg(long, default_value = "data.db")]
    db: PathBuf,
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
    let token_manager = TokenManager::new(oauth_client, store);
    let mut client = GmailClient::new(token_manager);
    let profile = client.profile().await?;

    println!("{profile:?}");

    Ok(())
}

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
}
