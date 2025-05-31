mod client;
mod oauth;
mod store;

use clap::Parser;
use oauth::{ClientCredentials, OAuthBroker};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let creds = ClientCredentials::load_from_file(args.path)?;
    let tokens = OAuthBroker::authorize(creds).await?;

    println!("{tokens:?}");

    Ok(())
}
