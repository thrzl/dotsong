use crate::http;
use crate::models;
use crate::models::CoverArtwork;
use moka::future::Cache;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use std::sync::LazyLock;

static CLEAN_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(?(feat\.|ft\.)\s.+\)?").unwrap());

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
    pub cover_artwork: Option<String>,
    pub isrc: Option<String>,
    pub duration: u64, // duration in seconds! important!
}

pub struct DeezerClient {
    cache: Cache<String, DeezerTrack>,
}

impl DeezerClient {
    pub fn new(cache_size: u64) -> Self {
        DeezerClient {
            cache: Cache::builder()
                .max_capacity(cache_size)
                .eviction_policy(moka::policy::EvictionPolicy::tiny_lfu())
                .build(),
        }
    }

    pub async fn track_search(
        &self,
        track: &models::MediaInfo,
        apple_music: bool,
    ) -> Option<DeezerTrack> {
        let clean_title = CLEAN_TITLE_RE
            .replace_all(track.title(), "")
            .trim()
            .to_string();
        let query = utf8_percent_encode(
            &format!("{} {} {}", clean_title, track.album(), track.artist()),
            NON_ALPHANUMERIC,
        )
        .to_string();
        if let Some(cached_track) = self.cache.get(&query).await {
            return Some(cached_track);
        }
        let url = format!("https://api.deezer.com/search?q={}", query);
        let response = http::client().get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let response_json: serde_json::Value = response.json().await.ok()?;
        let found_tracks = match response_json["data"].as_array() {
            Some(arr) => arr,
            None => return None,
        };
        let track_info = if apple_music {
            found_tracks.iter().find(|t| {
                // if it's apple music, the album title is in the artist field, so we need to check if the track artist contains the album title instead
                let artist = track.artist();
                if artist.is_empty() {
                    return false;
                }
                artist
                    .to_lowercase()
                    .contains(&t["album"]["title"].as_str().unwrap().to_lowercase())
            })
        } else {
            let mut tracks = found_tracks.iter().filter(|t| {
                let title_matches = t["title"].as_str().map(|s| s.to_lowercase())
                    == clean_title.to_lowercase().into();
                let deezer_artist = t["artist"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase();
                let track_artist = track.artist().to_lowercase();
                let artist_matches =
                    deezer_artist.contains(&track_artist) || track_artist.contains(&deezer_artist);
                title_matches && artist_matches
            });
            let final_track = if track.album.is_some() {
                tracks.find(|t| {
                    t["album"]["title"].as_str().map(|s| s.to_lowercase())
                        == track.album().to_lowercase().into()
                })
            } else {
                tracks.into_iter().next()
            };
            final_track
        };
        let track = Some(DeezerTrack {
            id: track_info?["id"].as_u64()?,
            title: track_info?["title"].as_str()?.to_string(),
            isrc: track_info?["isrc"].as_str().map(|s| s.to_string()),
            album: DeezerAlbum {
                id: track_info?["album"]["id"].as_u64()?,
                title: track_info?["album"]["title"].as_str()?.to_string(),
                cover_artwork: track_info?["album"]["cover_big"]
                    .as_str()
                    .map(|s| s.to_string()),
            },
            artist: track_info?["artist"]["name"].as_str()?.to_string(),
            cover_artwork: track_info?["album"]["cover"]
                .as_str()
                .map(|s| s.to_string()),
            duration: track_info?["duration"].as_u64().unwrap_or(0),
        });
        self.cache.insert(query, track.clone().unwrap()).await;
        track
    }

    pub async fn enrich_media_info(
        &self,
        media_info: &models::MediaInfo,
        apple_music: bool,
    ) -> Option<models::MediaInfo> {
        let enriched_track = self.track_search(media_info, apple_music).await?;
        // if it's apple music, trust deezer more than the player
        // apple music artist field may look like this: "artist name album name" with no delimiter.
        // so we'll go by character count instead of trying to split by a delimiter, which may not even be there
        let artist = if apple_music {
            let big_string = media_info.artist();
            if !big_string.is_empty() {
                big_string
                    .get(..enriched_track.artist.len())
                    .unwrap_or(&enriched_track.artist)
            } else {
                &enriched_track.artist
            }
        } else {
            &enriched_track.artist
        };
        let album = if apple_music {
            let big_string = media_info.album();
            if !big_string.is_empty() {
                big_string
                    .get(enriched_track.artist.len()..)
                    .unwrap_or(&enriched_track.album.title)
                    .trim()
            } else {
                &enriched_track.album.title
            }
        } else {
            &enriched_track.album.title
        };
        Some(models::MediaInfo {
            title: Some(media_info.title.clone().unwrap_or(enriched_track.title)),
            album: if apple_music {
                Some(album.to_string())
            } else {
                Some(
                    media_info
                        .album
                        .clone()
                        .unwrap_or(enriched_track.album.title),
                )
            },
            artist: if apple_music {
                Some(artist.to_string())
            } else {
                Some(media_info.artist.clone().unwrap_or(enriched_track.artist))
            },
            elapsed_time: media_info.elapsed_time,
            cover_artwork: Some(CoverArtwork::from_url(
                enriched_track
                    .cover_artwork
                    .unwrap_or_else(|| "default".to_string()),
            )),
            is_playing: media_info.is_playing,
            duration: if media_info.duration.is_some_and(|d| d == 0) {
                Some(enriched_track.duration as u32)
            } else {
                media_info.duration.or(Some(enriched_track.duration as u32))
            },
            isrc: enriched_track.isrc,
        })
    }
}
