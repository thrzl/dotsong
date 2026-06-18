pub mod deezer_api;

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub elapsed_time: Option<u32>,
    pub cover_artwork: Option<String>,
    pub is_playing: bool,
    pub duration: Option<u32>,
}
