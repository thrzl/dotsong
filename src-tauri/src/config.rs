use crate::scrobblers::{Scrobbler, ListenBrainzScrobbler, LastFMScrobbler};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub enum ScrobblerFormat {
    ListenBrainz,
    LastFM
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ScrobblerConfig {
    id: String,
    endpoint_url: String,
    api_key: String,
    format: ScrobblerFormat,
}

impl ScrobblerConfig {
    pub fn scrobbler(&self) -> Box<dyn Scrobbler + Send + Sync> {
        match self.format {
            ScrobblerFormat::ListenBrainz => Box::new(ListenBrainzScrobbler {
                endpoint_url: self.endpoint_url.clone(),
                api_key: self.api_key.clone(),
            }),
            ScrobblerFormat::LastFM => Box::new(LastFMScrobbler {
                endpoint_url: self.endpoint_url.clone(),
                api_key: self.api_key.clone(),
            }),
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub scrobblers: Vec<ScrobblerConfig>,
    pub discord_rpc_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scrobblers: Vec::new(),
            discord_rpc_enabled: false,
        }
    }
}