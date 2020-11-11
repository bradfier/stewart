use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, register_int_gauge, Encoder, IntCounterVec, IntGauge};
use warp::{Rejection, Reply};

lazy_static! {
    pub static ref PROTEST_CHANNELS_CREATED: IntCounterVec = register_int_counter_vec!(
        "stewart_protest_channels_created",
        "Number of protest channels created",
        &["user", "guild_name"]
    )
    .unwrap();
    pub static ref GUILDS_CONNECTED: IntGauge = register_int_gauge!(
        "stewart_guilds_connected",
        "Number of guilds to which the bot is connected"
    )
    .unwrap();
}

pub async fn metrics_handler() -> Result<impl Reply, Rejection> {
    let encoder = prometheus::TextEncoder::new();
    let metrics = prometheus::gather();
    let mut buffer = vec![];

    encoder
        .encode(&metrics, &mut buffer)
        .expect("Failed to encode metrics as text");

    Ok(buffer)
}
