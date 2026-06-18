use async_trait::async_trait;
use crate::models::MediaInfo;
use serde::{Serialize, Deserialize};

#[async_trait]
pub trait Scrobbler: Send + Sync {
    async fn scrobble(&self, track: &MediaInfo);
}

#[derive(Deserialize, Serialize)]
pub struct ListenBrainzScrobbler {
    pub endpoint_url: String,
    pub api_key: String,
}

#[async_trait]
impl Scrobbler for ListenBrainzScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement ListenBrainz scrobbling logic here
        track.duration; // quiet warnings about unused variable
    }
}

#[derive(Deserialize, Serialize)]
pub struct LastFMScrobbler {
    pub endpoint_url: String,
    pub api_key: String,
}

#[async_trait]
impl Scrobbler for LastFMScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement LastFM scrobbling logic here
        track.duration; // quiet warnings about unused variable
    }
}