use serde::Serialize;

// from rspotify
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListenType {
    Single,
    PlayingNow,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct Scrobble {
    pub listen_type: ListenType,
    pub payload: Vec<Payload>,
}

impl Scrobble {
    pub fn from_media_info(track: &crate::models::MediaInfo, listen_type: ListenType) -> Self {
        let payload = Payload {
            listened_at: match listen_type {
                ListenType::Single => Some(chrono::Utc::now().timestamp()),
                ListenType::PlayingNow => None,
            },
            track_metadata: TrackMetadata {
                additional_info: AdditionalInfo {
                    release_mbid: None,
                    artist_mbids: None,
                    recording_mbid: None,
                    artist_names: vec![track.artist.clone().unwrap_or_default()],
                    duration_ms: track.duration.unwrap_or_default() as i64 * 1000,
                    isrc: track.isrc.clone(),
                    submission_client: "dotsong".to_string(),
                },
                artist_name: track.artist.clone().unwrap_or_default(),
                track_name: track.title.clone().unwrap_or_default(),
                release_name: track.album.clone().unwrap_or_default(),
            },
        };
        Scrobble {
            listen_type,
            payload: vec![payload],
        }
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub struct Payload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listened_at: Option<i64>,
    pub track_metadata: TrackMetadata,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct TrackMetadata {
    pub additional_info: AdditionalInfo,
    pub artist_name: String,
    pub track_name: String,
    pub release_name: String,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct AdditionalInfo {
    pub release_mbid: Option<String>,
    pub artist_mbids: Option<Vec<String>>,
    pub recording_mbid: Option<String>,
    pub artist_names: Vec<String>,
    pub duration_ms: i64,
    pub isrc: Option<String>,
    pub submission_client: String,
}
