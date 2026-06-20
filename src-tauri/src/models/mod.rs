pub mod deezer_api;
pub mod listenbrainz;

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub elapsed_time: Option<u32>,
    pub cover_artwork: Option<String>,
    pub is_playing: bool,
    pub duration: Option<u32>,
    pub isrc: Option<String>,
}

impl Default for MediaInfo {
    fn default() -> Self {
        MediaInfo {
            title: None,
            album: None,
            artist: None,
            elapsed_time: None,
            cover_artwork: None,
            is_playing: false,
            duration: None,
            isrc: None,
        }
    }
}
