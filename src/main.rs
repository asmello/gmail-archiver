mod client;
mod oauth;
mod store;

use clap::Parser;
use oauth::{ClientCredentials, OAuthBroker};
use std::path::PathBuf;
use store::Store;

#[derive(Parser)]
struct Args {
    path: PathBuf,
    #[arg(long, default_value = "data.db")]
    db: PathBuf,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let store = Store::open(args.db)?;
    let tokens = match store.load_tokens()? {
        Some(tokens) => tokens,
        None => {
            let creds = ClientCredentials::load_from_file(args.path)?;
            let tokens = OAuthBroker::authorize(creds).await?;
            store.set_tokens(&tokens)?;
            tokens
        }
    };

    println!("{tokens:?}");

    Ok(())
}
