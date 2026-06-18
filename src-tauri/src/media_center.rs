use parking_lot::Mutex;
use crate::models::{MediaInfo, self};
use tokio::sync::broadcast;
use std::sync::{Arc};
use std::thread::sleep;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum TrackUpdateEvent {
    NewTrack(MediaInfo),
    PlaybackStateChange(MediaInfo),
}

pub struct MediaCenter {
    last_track: Arc<Mutex<Option<MediaInfo>>>,
    track_tx: broadcast::Sender<TrackUpdateEvent>
}

impl MediaCenter {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        MediaCenter {
            last_track: Arc::new(Mutex::new(None)),
            track_tx: tx
        }
    }

    pub fn get_rx(&self) -> broadcast::Receiver<TrackUpdateEvent> {
        self.track_tx.subscribe()
    }

    fn should_broadcast_track(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        let Some(previous) = previous else {
            return true;
        };

        if previous.title != current.title
            || previous.artist != current.artist
            || previous.is_playing != current.is_playing
        {
            return true;
        }

        match (previous.elapsed_time, current.elapsed_time) {
            (Some(previous_elapsed), Some(current_elapsed)) => {
                previous_elapsed.abs_diff(current_elapsed) >= 2
            }
            _ => false,
        }
    }

    fn should_refresh_tray_menu(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        let Some(previous) = previous else {
            return true;
        };

        previous.title != current.title || previous.artist != current.artist
    }


    #[cfg(target_os = "macos")]
    pub fn start_media_poller(self: Arc<Self>) {
        println!("starting media poller");
        let tx = self.track_tx.clone();
        tauri::async_runtime::spawn(async move {
            let mut deezer_client = models::deezer_api::DeezerClient::new(100);
            let now_playing = media_remote::NowPlayingPerl::new();
            loop {
                sleep(Duration::from_millis(500));
                let Some(media) = now_playing.get_info().clone() else {
                    continue;
                };
                if media.title.is_none() && media.album.is_none() {
                    continue;
                }

                let media_info = MediaInfo {
                    title: media.title,
                    album: media
                        .album
                        .map(|album| Self::sanitize_apple_music_album_name(&album)),
                    artist: media.artist,
                    elapsed_time: media.elapsed_time.map(|t| t as u32),
                    cover_artwork: None,
                    is_playing: media.is_playing.unwrap_or(false),
                    duration: media.duration.map(|t| t as u64),
                };

                // asynchronous enriching of media info with Deezer API
                let media_info_clone = media_info.clone();
                let enriched_track = deezer_client.enrich_media_info(&media_info_clone).await;

                if !Self::should_broadcast_track(self.last_track.lock().as_ref(), &enriched_track) {
                    continue;
                }
                {
                    {
                        let mut last_track = self.last_track.lock();
                        *last_track = Some(enriched_track.clone());
                    }
                }
                
                tx.send(TrackUpdateEvent::NewTrack(enriched_track.clone())).unwrap();
                if Self::should_refresh_tray_menu(self.last_track.lock().as_ref(), &enriched_track) {
                    tx.send(TrackUpdateEvent::PlaybackStateChange(enriched_track)).unwrap();
                }
            }
        });
    }

    fn sanitize_apple_music_album_name(album_name: &str) -> String {
        let patterns = [" - Single", " - EP"];

        let mut sanitized_name = album_name.to_string();
        for pattern in patterns {
            sanitized_name = sanitized_name.replace(pattern, "");
        }
        sanitized_name.trim().to_string()
    }
}