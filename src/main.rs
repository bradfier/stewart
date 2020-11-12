use log::info;

use futures::{try_join, FutureExt};
use warp::Filter;

mod config;
mod discord;
mod metrics;
mod strategy;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let mut discord_client = discord::create_client(&config::CONFIG.discord_token).await;
    info!("Created Discord client successfully");

    // start listening for events by starting a single shard
    let discord_shard = discord_client.start();

    // Set up the HTTP handlers
    let metrics_route = warp::path!("metrics").and_then(metrics::metrics_handler);
    let warp_future = warp::serve(metrics_route)
        .bind(([0, 0, 0, 0], 5000))
        .map(Ok);

    if let Err(e) = try_join!(discord_shard, warp_future) {
        println!("An error occured when running the discord client: {:?}", e);
    }
}
