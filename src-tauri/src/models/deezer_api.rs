use crate::models;
use lfu::LFUCache;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

#[derive(Debug, Clone)]
pub struct DeezerAlbum {
    pub id: u64,
    pub title: String,
    pub cover_artwork: Option<String>, // this is cover_big from the API
}

#[derive(Debug, Clone)]
pub struct DeezerArtist {
    pub id: u64,
    pub name: String,
    pub picture: Option<String>, // this is picture_medium from the API
}       

#[derive(Debug, Clone)]
pub struct DeezerTrack {
    pub id: u64,
    pub title: String,
    pub album: DeezerAlbum,
    pub artist: String,
    pub elapsed_time: Option<u32>,
    pub cover_artwork: Option<String>,
    pub isrc: Option<String>,
    pub duration: u64 // duration in seconds! important!
}

pub struct DeezerClient {
    cache: LFUCache<String, DeezerTrack>,
}

impl DeezerClient {
    pub fn new(cache_size: usize) -> Self {
        DeezerClient { cache: LFUCache::with_capacity(cache_size).expect("couldn't create LFU cache") }
    }

    pub async fn track_search(&mut self, track: &models::MediaInfo) -> Option<DeezerTrack> {
        let query = utf8_percent_encode(&format!("{} {} {}", track.title.clone().unwrap_or_default(), track.album.clone().unwrap_or_default(), track.artist.clone().unwrap_or_default()), NON_ALPHANUMERIC).to_string();
        if let Some(cached_track) = self.cache.get(&query) {
            return Some(cached_track.clone());
        }
        let url = format!("https://api.deezer.com/search?q={}", query);
        let response = reqwest::get(url).await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let response_json: serde_json::Value = response.json().await.ok()?;
        let found_tracks = match response_json["data"].as_array() {
            Some(arr) => arr,
            None => return None,
        };
        let track_info = found_tracks.iter().find(|t| {
            t["album"]["title"].as_str().map(|s| s.to_lowercase()) == track.album.clone().unwrap_or_default().to_lowercase().into()
        })?;
        let track = Some(DeezerTrack {
            id: track_info["id"].as_u64()?,
            title: track_info["title"].as_str()?.to_string(),
            isrc: track_info["isrc"].as_str().map(|s| s.to_string()),
            album: DeezerAlbum { id: track_info["album"]["id"].as_u64()?, title: track_info["album"]["title"].as_str()?.to_string(), cover_artwork: track_info["album"]["cover_big"].as_str().map(|s| s.to_string()) },
            artist: track_info["artist"]["name"].as_str()?.to_string(),
            elapsed_time: track_info["elapsed_time"].as_u64().map(|t| t as u32),
            cover_artwork: track_info["album"]["cover"].as_str().map(|s| s.to_string()),
            duration: track_info["duration"].as_u64().unwrap_or(0),
        });
        self.cache.set(query, track.clone().unwrap());
        track
    }
}