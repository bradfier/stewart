use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashSet;
use std::net::SocketAddr;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub discord_token: String,
    #[serde(default)]
    pub protest_channels: HashSet<u64>,
    pub socket_addr: SocketAddr,
}

lazy_static! {
    pub static ref CONFIG: Config = match envy::from_env() {
        Ok(config) => config,
        Err(e) => panic!("Failed to load config: {:#?}", e),
    };
}
