use async_trait::async_trait;
use crate::models::MediaInfo;

pub enum ScrobblerFormat {
    ListenBrainz,
    LastFM
}

#[async_trait]
pub trait Scrobbler: Send + Sync {
    async fn scrobble(&self, track: &MediaInfo);
}

pub struct ListenBrainzScrobbler {
    endpoint_url: String,
    api_key: String,
}

#[async_trait]
impl Scrobbler for ListenBrainzScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement ListenBrainz scrobbling logic here
        track; // quiet warnings about unused variable
    }
}

pub struct LastFMScrobbler {
    endpoint_url: String,
    api_key: String,
}

#[async_trait]
impl Scrobbler for LastFMScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement LastFM scrobbling logic here
        track; // quiet warnings about unused variable
    }
}

pub struct ScrobblerConfig {
    endpoint_url: String,
    api_key: String,
    format: ScrobblerFormat,
}

impl ScrobblerConfig {
    fn scrobbler(&self) -> Box<dyn Scrobbler + Send + Sync> {
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

pub struct Config {
    scrobblers: Vec<ScrobblerConfig>,
    discord_rpc_enabled: bool,
}