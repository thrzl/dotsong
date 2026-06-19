use crate::models::{listenbrainz, MediaInfo};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub enum ScrobblerFormat {
    ListenBrainz,
    LastFM,
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
        println!("scrobbling to {}: {} - {}", self.endpoint_url.trim_end_matches("/"), track.artist.clone().unwrap_or_default(), track.title.clone().unwrap_or_default());
        let scrobble =
            listenbrainz::Scrobble::from_media_info(track, listenbrainz::ListenType::Single);
        reqwest::Client::new()
            .post(format!("{}/submit-listens", self.endpoint_url.trim_end_matches("/")))
            .header("Authorization", format!("Token {}", self.api_key))
            .json(&scrobble)
            .send()
            .await
            .map_err(|e| eprintln!("failed to send scrobble to ListenBrainz: {}", e)).ok();
    }

    async fn scrobble_lastfm(&self, track: &MediaInfo) {
        // implement LastFM scrobbling logic here
        println!("scrobbling to {}: {} - {}", self.endpoint_url.trim_end_matches("/"), track.artist.clone().unwrap_or_default(), track.title.clone().unwrap_or_default());
        track.duration; // quiet warnings about unused variable
    }

    async fn now_playing_listenbrainz(&self, track: &MediaInfo) {
        // implement ListenBrainz now playing logic here
        println!("Sending now playing to {}: {} - {}", self.endpoint_url.trim_end_matches("/"), track.artist.clone().unwrap_or_default(), track.title.clone().unwrap_or_default());
        let scrobble =
            listenbrainz::Scrobble::from_media_info(track, listenbrainz::ListenType::PlayingNow); // quiet warnings about unused variable
        let res = reqwest::Client::new()
            .post(format!("{}/submit-listens", self.endpoint_url.trim_end_matches("/")))
            .header("Authorization", format!("Token {}", self.api_key))
            .json(&scrobble)
            .send()
            .await;
        match res {
            Ok(response) => {
                if !response.status().is_success() {
                    eprintln!("Failed to send now playing to {}: HTTP {}\n{}", self.endpoint_url.trim_end_matches("/"), response.status(), serde_json::to_string_pretty(&response.text().await.unwrap()).unwrap());
                }
            }
            Err(e) => {
                eprintln!("Failed to send now playing to {}: {}", self.endpoint_url.trim_end_matches("/"), e);
            }
        }
    }

    async fn now_playing_lastfm(&self, track: &MediaInfo) {
        // implement LastFM now playing logic here
        println!("Sending now playing to {}: {} - {}", self.endpoint_url.trim_end_matches("/"), track.artist.clone().unwrap_or_default(), track.title.clone().unwrap_or_default());
        track.duration; // quiet warnings about unused variable
    }

    pub async fn scrobble(&self, track: &MediaInfo) {
        match self.format {
            ScrobblerFormat::ListenBrainz => {self.scrobble_listenbrainz(track).await;},
            ScrobblerFormat::LastFM => {self.scrobble_lastfm(track).await;},
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
