use crate::http;
use bytes::Bytes;

pub mod deezer_api;
pub mod listenbrainz;

#[derive(Debug, Clone)]
pub struct CoverArtwork {
    data: Option<Bytes>,
    url: Option<String>,
}

impl CoverArtwork {
    pub fn bytes(&self) -> Option<Bytes> {
        if let Some(data) = &self.data {
            return Some(data.clone());
        };
        return None;
    }

    /// always returns a slice
    /// requires a mutable reference
    /// because it will store the bytes in the struct if they are fetched from the url
    pub async fn fetch_bytes(&mut self) -> Result<Bytes, reqwest::Error> {
        if let Some(data) = &self.data {
            return Ok(data.clone());
        };
        if let Some(url) = &self.url {
            if let Ok(response) = http::client().get(url).send().await {
                if response.status().is_success() {
                    if let Ok(bytes) = response.bytes().await {
                        let bytes = Bytes::from(bytes);
                        self.data.replace(bytes.clone());
                        return Ok(bytes);
                    }
                }
            }
        }
        // will never happen
        panic!("there should be a url or data for the cover image, but there is neither")
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

    pub fn from_bytes(bytes: Bytes) -> Self {
        CoverArtwork {
            data: Some(bytes),
            url: None,
        }
    }

    pub fn set_url(&mut self, url: String) {
        self.url = Some(url);
        self.data = None;
    }

    pub async fn upload_bytes(&mut self) -> Result<String, reqwest::Error> {
        let bytes = self.data.clone().unwrap_or_else(|| Bytes::new());
        let form = reqwest::multipart::Form::new()
            .text("reqtype", "fileupload")
            .text("fileNameLength", "16")
            .text("time", "12h")
            .part(
                "fileToUpload",
                reqwest::multipart::Part::bytes(bytes.to_vec())
                    .file_name("cover_image.jpg")
                    .mime_str("image/jpg")
                    .unwrap(),
            );
        println!("uploading cover artwork to litterbox");
        let res = crate::http::client()
            .post("https://litterbox.catbox.moe/resources/internals/api.php")
            .multipart(form)
            .send()
            .await?;

        let url = res.text().await?;
        self.url = Some(url.clone());
        Ok(url)
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

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub elapsed_time: Option<u32>,
    pub cover_artwork: Option<CoverArtwork>,
    pub is_playing: bool,
    pub duration: Option<u32>,
    pub isrc: Option<String>,
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
