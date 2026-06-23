use crate::lastfm_auth;
use crate::models::{listenbrainz, MediaInfo};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub enum ScrobblerFormat {
    ListenBrainz,
    LastFM,
    LibreFM,
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
        println!(
            "scrobbling to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        let scrobble =
            listenbrainz::Scrobble::from_media_info(track, listenbrainz::ListenType::Single);
        crate::http::client()
            .post(format!(
                "{}/submit-listens",
                self.endpoint_url.trim_end_matches("/")
            ))
            .header("Authorization", format!("Token {}", self.api_key))
            .json(&scrobble)
            .send()
            .await
            .map_err(|e| eprintln!("failed to send scrobble to ListenBrainz: {}", e))
            .ok();
    }

    async fn scrobble_lastfm(&self, track: &MediaInfo) {
        use last_fm_rs::{Client, Scrobble};
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = Client::new(lastfm_auth::LASTFM_API_KEY, lastfm_auth::LASTFM_API_SECRET)
            .with_session_key(&self.api_key);
        let scrobble = Scrobble::new(
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default(),
            timestamp,
        )
        .with_album(track.album.clone().unwrap_or_default())
        .with_duration(track.duration.unwrap_or_default().into());
        // implement LastFM scrobbling logic here
        println!(
            "scrobbling to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        match client.scrobble(&[scrobble]).await {
            Ok(_) => (),
            Err(e) => eprintln!("failed to send scrobble to LastFM: {}", e),
        };
    }

    async fn now_playing_listenbrainz(&self, track: &MediaInfo) {
        // implement ListenBrainz now playing logic here
        println!(
            "sending now playing to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        let scrobble =
            listenbrainz::Scrobble::from_media_info(track, listenbrainz::ListenType::PlayingNow); // quiet warnings about unused variable
        let res = crate::http::client()
            .post(format!(
                "{}/submit-listens",
                self.endpoint_url.trim_end_matches("/")
            ))
            .header("Authorization", format!("Token {}", self.api_key))
            .json(&scrobble)
            .send()
            .await;
        match res {
            Ok(response) => {
                if !response.status().is_success() {
                    eprintln!(
                        "Failed to send now playing to {}: HTTP {}\n{}",
                        self.endpoint_url.trim_end_matches("/"),
                        response.status(),
                        serde_json::to_string_pretty(&response.text().await.unwrap()).unwrap()
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to send now playing to {}: {}",
                    self.endpoint_url.trim_end_matches("/"),
                    e
                );
            }
        }
    }

    async fn now_playing_lastfm(&self, track: &MediaInfo) {
        use last_fm_rs::{Client, NowPlaying};
        println!(
            "sending now playing to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        let client = Client::new(lastfm_auth::LASTFM_API_KEY, lastfm_auth::LASTFM_API_SECRET)
            .with_session_key(&self.api_key);
        let now_playing = NowPlaying::new(
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default(),
        )
        .with_album(track.album.clone().unwrap_or_default())
        .with_duration(track.duration.unwrap_or_default().into());
        match client.update_now_playing(&now_playing).await {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to send now playing to LastFM: {}", e),
        };
    }

    async fn scrobble_librefm(&self, track: &MediaInfo) {
        use last_fm_rs::{Client, Scrobble};
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = match Client::new(
            lastfm_auth::LIBREFM_API_KEY,
            lastfm_auth::LIBREFM_API_SECRET,
        )
        .with_api_base(&self.endpoint_url)
        {
            Ok(c) => c.with_session_key(&self.api_key),
            Err(e) => {
                eprintln!("invalid libre.fm endpoint url: {}", e);
                return;
            }
        };
        let scrobble = Scrobble::new(
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default(),
            timestamp,
        )
        .with_album(track.album.clone().unwrap_or_default())
        .with_duration(track.duration.unwrap_or_default().into());
        println!(
            "scrobbling to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        match client.scrobble(&[scrobble]).await {
            Ok(_) => (),
            Err(e) => eprintln!("failed to send scrobble to LibreFM: {}", e),
        };
    }

    async fn now_playing_librefm(&self, track: &MediaInfo) {
        use last_fm_rs::{Client, NowPlaying};
        println!(
            "sending now playing to {}: {} - {}",
            self.endpoint_url.trim_end_matches("/"),
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default()
        );
        let client = match Client::new(
            lastfm_auth::LIBREFM_API_KEY,
            lastfm_auth::LIBREFM_API_SECRET,
        )
        .with_api_base(&self.endpoint_url)
        {
            Ok(c) => c.with_session_key(&self.api_key),
            Err(e) => {
                eprintln!("invalid libre.fm endpoint url: {}", e);
                return;
            }
        };
        let now_playing = NowPlaying::new(
            track.artist.clone().unwrap_or_default(),
            track.title.clone().unwrap_or_default(),
        )
        .with_album(track.album.clone().unwrap_or_default())
        .with_duration(track.duration.unwrap_or_default().into());
        match client.update_now_playing(&now_playing).await {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to send now playing to LibreFM: {}", e),
        };
    }

    pub async fn scrobble(&self, track: &MediaInfo) {
        match self.format {
            ScrobblerFormat::ListenBrainz => {
                self.scrobble_listenbrainz(track).await;
            }
            ScrobblerFormat::LastFM => {
                self.scrobble_lastfm(track).await;
            }
            ScrobblerFormat::LibreFM => {
                self.scrobble_librefm(track).await;
            }
        }
    }

    pub async fn now_playing(&self, track: &MediaInfo) {
        match self.format {
            ScrobblerFormat::ListenBrainz => self.now_playing_listenbrainz(track).await,
            ScrobblerFormat::LastFM => self.now_playing_lastfm(track).await,
            ScrobblerFormat::LibreFM => self.now_playing_librefm(track).await,
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
