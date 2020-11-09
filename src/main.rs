use log::info;
use std::env;

mod discord;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in the environment");
    let mut discord_client = discord::create_client(&token).await;
    info!("Created Discord client successfully");

    // start listening for events by starting a single shard
    if let Err(e) = discord_client.start().await {
        println!("An error occurred while running the client: {:?}", e);
    }
}
