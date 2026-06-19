use serde::Deserialize;

#[derive(Deserialize, Clone)]
struct CreditsFMResolution {
    isrc: Option<String>,
    recording_title: Option<String>,
    artist_names: Option<Vec<String>>,
    duration: Option<u32>,
}