use crate::http;
use crate::models;
use crate::models::CoverArtwork;
use log::{debug, warn};
use mini_moka::sync::Cache;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;
use unicase::UniCase;

static CLEAN_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(?(feat\.|ft\.)\s.+\)?").unwrap());

#[derive(Deserialize, Debug, Clone)]
pub struct DeezerAlbum {
    pub id: u64,
    pub title: String,
    pub cover_big: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DeezerArtist {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DeezerTrack {
    pub id: u64,
    pub title: String,
    pub album: DeezerAlbum,
    pub artist: DeezerArtist,
    pub isrc: Option<String>,
    pub duration: u64, // duration in seconds! important!
}

#[derive(Deserialize)]
pub struct DeezerSearchResponse {
    pub data: Vec<DeezerTrack>,
}

pub struct DeezerClient {
    cache: Cache<String, DeezerTrack>,
}

impl DeezerClient {
    pub fn new(cache_size: u64) -> Self {
        DeezerClient {
            cache: Cache::builder().max_capacity(cache_size).build(),
        }
    }

    pub async fn track_search(
        &self,
        track: &models::MediaInfo,
        apple_music: bool,
    ) -> Option<DeezerTrack> {
        let clean_title = CLEAN_TITLE_RE.replace_all(track.title(), "");
        let clean_title = clean_title.trim();
        let query = utf8_percent_encode(
            &format!(
                "{} {} {}",
                clean_title,
                track.album(),
                track.artist().trim_end_matches(" - Topic")
            ),
            NON_ALPHANUMERIC,
        )
        .to_string();
        if let Some(cached_track) = self.cache.get(&query) {
            return Some(cached_track);
        }
        let url = format!("https://api.deezer.com/search?q={}", query);
        let response = http::client().get(url).send().await.ok()?;
        if !response.status().is_success() {
            warn!(
                "deezer track search failed for query: {} with status: {}",
                query,
                response.status()
            );
            return None;
        }
        let response_json: DeezerSearchResponse = response.json().await.ok()?;
        let found_tracks = if response_json.data.len() > 0 {
            response_json.data
        } else {
            debug!(
                "deezer track search returned no results for query: {}",
                query
            );
            return None;
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
                    .contains(&t.album.title.to_lowercase())
            })
        } else {
            let mut tracks = found_tracks.iter().filter(|found_track| {
                let title_matches =
                    UniCase::new(CLEAN_TITLE_RE.replace(&found_track.title, "").trim())
                        == UniCase::new(&clean_title);
                let deezer_artist = found_track.artist.name.to_lowercase();
                let track_artist = track.artist().to_lowercase();
                let artist_matches =
                    deezer_artist.contains(&track_artist) || track_artist.contains(&deezer_artist);
                title_matches && artist_matches
            });
            let final_track = if track.album.as_ref().is_some_and(|a| !a.is_empty()) {
                tracks.find(|t| {
                    UniCase::new(CLEAN_TITLE_RE.replace(&t.album.title, "").trim())
                        == UniCase::new(CLEAN_TITLE_RE.replace(track.album(), "").trim())
                })
            } else {
                tracks.into_iter().next()
            };
            final_track
        };
        debug!(
            "deezer track search for query: {} found: {:?}",
            query, track_info
        );
        let track = Some(DeezerTrack {
            id: track_info?.id,
            title: track_info?.title.clone(),
            isrc: track_info?.isrc.clone(),
            album: DeezerAlbum {
                id: track_info?.album.id,
                title: track_info?.album.title.clone(),
                cover_big: track_info?.album.cover_big.clone(),
            },
            artist: DeezerArtist {
                id: track_info?.artist.id,
                name: track_info?.artist.name.clone(),
            },
            duration: track_info?.duration,
        });
        self.cache.insert(query, track.clone().unwrap());
        track
    }

    pub async fn enrich_media_info(
        &self,
        media_info: &models::MediaInfo,
    ) -> Option<models::MediaInfo> {
        let apple_music = media_info.is_apple_music();
        let enriched_track = self.track_search(media_info, apple_music).await?;
        // if it's apple music, trust deezer more than the player
        // apple music artist field may look like this: "artist name album name" with no delimiter.
        // so we'll go by character count instead of trying to split by a delimiter, which may not even be there
        let artist = if apple_music {
            let big_string = media_info.artist();
            if !big_string.is_empty() {
                big_string
                    .get(..enriched_track.artist.name.len())
                    .unwrap_or(&enriched_track.artist.name)
            } else {
                &enriched_track.artist.name
            }
        } else {
            &enriched_track.artist.name
        };
        let album = if apple_music {
            let big_string = media_info.album();
            if !big_string.is_empty() {
                big_string
                    .get(enriched_track.artist.name.len()..)
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
                        .and_then(|a| if a.is_empty() { None } else { Some(a) })
                        .unwrap_or(enriched_track.album.title),
                )
            },
            artist: if apple_music {
                Some(artist.to_string())
            } else {
                Some(
                    media_info
                        .artist
                        .clone()
                        .unwrap_or(enriched_track.artist.name),
                )
            },
            elapsed_time: media_info.elapsed_time,
            cover_artwork: Some(CoverArtwork::from_url(
                enriched_track
                    .album
                    .cover_big
                    .unwrap_or_else(|| "default".to_string()),
            )),
            is_playing: media_info.is_playing,
            duration: if media_info.duration.is_some_and(|d| d == 0) {
                Some(enriched_track.duration as u32)
            } else {
                media_info.duration.or(Some(enriched_track.duration as u32))
            },
            isrc: enriched_track.isrc,
            player_name: media_info.player_name.clone(),
        })
    }
}
