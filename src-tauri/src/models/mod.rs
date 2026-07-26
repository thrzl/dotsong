use crate::media_center::BROWSERS;
use bytes::Bytes;
use image::DynamicImage;
use log::{debug, info};
use mini_moka::sync::Cache;
use std::sync::LazyLock;
use xxhash_rust::xxh3::xxh3_64;

static LITTERBOX_CACHE: LazyLock<Cache<u64, String>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100)
        .time_to_live(std::time::Duration::from_hours(1))
        .build()
});

pub mod deezer_api;
pub mod listenbrainz;

#[derive(Debug, Clone, PartialEq)]
pub struct CoverArtwork {
    data: Option<Bytes>,
    url: Option<String>,
}

impl CoverArtwork {
    pub fn bytes(&self) -> Option<&Bytes> {
        if let Some(data) = &self.data {
            return Some(data);
        };
        return None;
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn from_url(url: String) -> Self {
        CoverArtwork {
            data: None,
            url: Some(url),
        }
    }

    pub fn from_dynamic_image(image: &DynamicImage) -> Self {
        let rgb8 = image.thumbnail(512, 512).to_rgb8();
        let mut buf = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut buf)
            .encode_image(&rgb8)
            .unwrap();
        let bytes = Bytes::from(buf);
        CoverArtwork {
            data: Some(bytes),
            url: None,
        }
    }

    pub async fn into_uploaded(&self) -> Result<Self, reqwest::Error> {
        let Some(bytes) = self.data.as_ref() else {
            return Ok(Self::from_url("default".to_string()));
        };
        let hash = xxh3_64(&bytes);
        if let Some(cached_url) = LITTERBOX_CACHE.get(&hash) {
            debug!("already uploaded image {:016x}, cache hit", hash);
            return Ok(Self::from_url(cached_url.clone()));
        }
        let form = reqwest::multipart::Form::new()
            .text("reqtype", "fileupload")
            .text("fileNameLength", "16")
            .text("time", "1h")
            .part(
                "fileToUpload",
                reqwest::multipart::Part::stream(bytes.clone())
                    .file_name("cover_image.jpg")
                    .mime_str("image/jpg")
                    .unwrap(),
            );
        info!("uploading cover artwork to litterbox");
        let res = crate::http::client()
            .post("https://litterbox.catbox.moe/resources/internals/api.php")
            .multipart(form)
            .send()
            .await?;

        let url = res.text().await?;
        LITTERBOX_CACHE.insert(hash, url.clone());
        Ok(Self::from_url(url))
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn from_nowhear_artwork(artwork: nowhear::Artwork) -> Option<Self> {
        match artwork {
            nowhear::Artwork::Url { url } => {
                if url.starts_with("file://") {
                    let path = url.trim_start_matches("file://");
                    if let Ok(image) = image::open(path) {
                        return Some(CoverArtwork::from_dynamic_image(&image));
                    } else {
                        return None;
                    }
                } else {
                    Some(CoverArtwork::from_url(url))
                }
            }
            nowhear::Artwork::Bytes { mime, data } => {
                if mime.is_some_and(|m| m != "image/jpeg") {
                    // if it's not a jpeg, we need to convert it with image crate
                    let img = image::load_from_memory(&*data)
                        .expect("should be able to load image from bytes");
                    Some(CoverArtwork::from_dynamic_image(&img))
                } else {
                    Some(CoverArtwork {
                        data: Some(Bytes::from(data.to_vec())),
                        url: None,
                    })
                }
            }
        }
    }
}

impl Default for CoverArtwork {
    fn default() -> Self {
        CoverArtwork {
            data: None,
            url: Some("default".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub elapsed_time: Option<u32>,
    pub cover_artwork: Option<CoverArtwork>,
    pub is_playing: bool,
    pub duration: Option<u32>,
    pub isrc: Option<String>,
    pub player_name: Option<String>,
}

impl MediaInfo {
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or_default()
    }
    pub fn artist(&self) -> &str {
        self.artist.as_deref().unwrap_or_default()
    }
    pub fn album(&self) -> &str {
        self.album.as_deref().unwrap_or_default()
    }
    pub fn is_apple_music(&self) -> bool {
        let Some(name) = &self.player_name else {
            return false;
        };
        let name = name.to_lowercase();
        name.contains("apple") && name.contains("music")
    }
    pub fn is_browser(&self) -> bool {
        match &self.player_name {
            Some(name) => {
                let name = {
                    #[cfg(target_os = "windows")]
                    name.strip_suffix(".exe").unwrap_or(name);
                    #[cfg(not(target_os = "windows"))]
                    name
                };
                BROWSERS
                    .iter()
                    .any(|&browser| browser.eq_ignore_ascii_case(&name))
            }
            None => false,
        }
    }
}

impl Default for MediaInfo {
    fn default() -> Self {
        MediaInfo {
            title: None,
            album: None,
            artist: None,
            elapsed_time: None,
            cover_artwork: None,
            player_name: None,
            is_playing: false,
            duration: None,
            isrc: None,
        }
    }
}
