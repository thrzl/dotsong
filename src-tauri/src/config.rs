use serde::{Deserialize, Serialize};
use crate::models::MediaInfo;

#[derive(Deserialize, Serialize, Clone)]
pub enum ScrobblerFormat {
    ListenBrainz,
    LastFM
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Scrobbler {
    id: String,
    endpoint_url: String,
    api_key: String,
    format: ScrobblerFormat,
}

impl Scrobbler {
    async fn scrobble_listenbrainz(&self, track: &MediaInfo) {
        // implement ListenBrainz scrobbling logic here
        track.duration; // quiet warnings about unused variable
    }
    async fn scrobble_lastfm(&self, track: &MediaInfo) {
        // implement LastFM scrobbling logic here
        track.duration; // quiet warnings about unused variable
    }

    async fn now_playing_listenbrainz(&self, track: &MediaInfo) {
        // implement ListenBrainz now playing logic here
        track.duration; // quiet warnings about unused variable
    }

    async fn now_playing_lastfm(&self, track: &MediaInfo) {
        // implement LastFM now playing logic here
        track.duration; // quiet warnings about unused variable
    }

    pub async fn scrobble(&self, track: &MediaInfo) {
        match self.format {
            ScrobblerFormat::ListenBrainz => self.scrobble_listenbrainz(track).await,
            ScrobblerFormat::LastFM => self.scrobble_lastfm(track).await,
        }
    }

    pub async fn now_playing(&self, track: &MediaInfo) {
        match self.format {
            ScrobblerFormat::ListenBrainz => self.now_playing_listenbrainz(track).await,
            ScrobblerFormat::LastFM => self.now_playing_lastfm(track).await,
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub scrobblers: Vec<Scrobbler>,
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