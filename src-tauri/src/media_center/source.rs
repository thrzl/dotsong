use std::sync::Arc;

#[cfg(target_os = "macos")]
use tokio::sync::mpsc;

#[cfg(any(target_os = "linux", target_os = "windows"))]
use futures::StreamExt;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use nowhear::MediaSource;

use crate::models::{CoverArtwork, MediaInfo};

/// A platform-agnostic async iterator over media events. Each platform supplies
/// its own concrete implementation; the shared `MediaCenter` poller just drives
/// `next_event` in a loop.
#[async_trait::async_trait]
pub trait OsMediaSource: Send + Sync {
    async fn next_event(&self) -> Option<MediaInfo>;
}

/// Build the right `OsMediaSource` for the current platform.
pub fn create() -> Arc<dyn OsMediaSource> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(MacMediaSource::new())
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        Arc::new(NowHearSource::new())
    }
}

// ---------------------------------------------------------------------------
// macOS: media-remote adapter (sync callback) -> mpsc channel -> async
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
use media_remote::Subscription;

#[cfg(target_os = "macos")]
struct MacMediaSource {
    rx: tokio::sync::Mutex<mpsc::Receiver<MediaInfo>>,
    _player: media_remote::NowPlayingPerl,
}

#[cfg(target_os = "macos")]
impl MacMediaSource {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel::<MediaInfo>(16);
        let player = media_remote::NowPlayingPerl::new();
        player.subscribe(move |guard| {
            let tx = tx.clone();
            let Some(info) = guard.as_ref() else { return };
            let cover_artwork = info
                .album_cover
                .as_ref()
                .map(CoverArtwork::from_dynamic_image);
            let media_info = MediaInfo {
                title: info.title.clone(),
                album: info
                    .album
                    .clone()
                    .map(|album| sanitize_apple_music_album_name(&album)),
                artist: info.artist.clone(),
                elapsed_time: info.elapsed_time.map(|t| t as u32),
                cover_artwork,
                is_playing: info.is_playing.unwrap_or(false),
                duration: info.duration.map(|t| t as u32),
                isrc: None,
                player_name: info.bundle_name.clone(),
            };
            // mac separates artist/album already; no need for the deezer
            // character-count split that the nowhear apple-music path needs.
            // The media-remote callback runs on its own thread, so we can't
            // `.await` here. `try_send` is sync and drops on a full channel;
            // events come fast enough that the next one will catch us up.
            println!("broadcasting new event (macOS)");
            tauri::async_runtime::spawn(async move {
                tx.send(media_info).await.unwrap();
            });
        });
        Self {
            rx: tokio::sync::Mutex::new(rx),
            _player: player,
        }
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl OsMediaSource for MacMediaSource {
    async fn next_event(&self) -> Option<MediaInfo> {
        println!("waiting for next event");
        let mut rx = self.rx.lock().await;
        rx.recv().await
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct NowHearSource {
    source: tokio::sync::Mutex<Option<nowhear::source::PlatformMediaSource>>,
    stream: tokio::sync::Mutex<Option<nowhear::source::EventStream>>,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl NowHearSource {
    fn new() -> Self {
        Self {
            source: tokio::sync::Mutex::new(None),
            stream: tokio::sync::Mutex::new(None),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[async_trait::async_trait]
impl OsMediaSource for NowHearSource {
    async fn next_event(&self) -> Option<MediaInfo> {
        let mut source_guard = self.source.lock().await;
        if source_guard.is_none() {
            match nowhear::MediaSourceBuilder::new().build().await {
                Ok(s) => *source_guard = Some(s),
                Err(e) => {
                    eprintln!("failed to build nowhear media source: {e}");
                    return None;
                }
            }
        }
        let source = source_guard.as_ref().unwrap();

        let mut stream_guard = self.stream.lock().await;
        if stream_guard.is_none() {
            match source.event_stream().await {
                Ok(s) => *stream_guard = Some(s),
                Err(e) => {
                    eprintln!("failed to open nowhear event stream: {e}");
                    return None;
                }
            }
        }
        let stream = stream_guard.as_mut().unwrap();
        let event = stream.next().await?;
        build_event(source, event).await
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn build_event(
    source: &nowhear::source::PlatformMediaSource,
    event: nowhear::MediaEvent,
) -> Option<MediaInfo> {
    match event {
        nowhear::MediaEvent::TrackChanged { player_name, track } => {
            let player = source.get_player(&player_name).await.ok()?;
            let is_apple_music = player_name.to_lowercase().contains("applemusic");
            let artist = if is_apple_music {
                // apple music on windows stuffs the album into the artist field,
                // separated by an em dash; we have to split it here.
                vec![track.artist[0].replace(" — ", " ")]
            } else {
                track.artist
            };
            Some(MediaInfo {
                title: Some(track.title),
                album: track
                    .album
                    .map(|album| sanitize_apple_music_album_name(&album)),
                artist: Some(artist.join(", ")),
                elapsed_time: Some(0),
                cover_artwork: track
                    .artwork
                    .and_then(|art| CoverArtwork::from_nowhear_artwork(art)),
                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                duration: track.duration.map(|t| t.as_secs() as u32),
                isrc: None,
                player_name: Some(player_name),
            })
        }
        nowhear::MediaEvent::PositionChanged {
            player_name,
            position,
        } => {
            let player = source.get_player(&player_name).await.ok()?;
            let is_apple_music = player_name.to_lowercase().contains("applemusic");
            let Some(track) = player.current_track else {
                return Some(MediaInfo {
                    title: None,
                    album: None,
                    artist: None,
                    elapsed_time: Some(position.as_secs() as u32),
                    cover_artwork: None,
                    is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                    duration: None,
                    isrc: None,
                    player_name: Some(player_name),
                });
            };
            let artist = if is_apple_music {
                vec![track.artist[0].replace(" — ", " ")]
            } else {
                track.artist
            };
            Some(MediaInfo {
                title: Some(track.title),
                album: track
                    .album
                    .map(|album| sanitize_apple_music_album_name(&album)),
                artist: Some(artist.join(", ")),
                elapsed_time: Some(position.as_secs() as u32),
                cover_artwork: track
                    .artwork
                    .and_then(|art| CoverArtwork::from_nowhear_artwork(art)),
                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                duration: track.duration.map(|t| t.as_secs() as u32),
                isrc: None,
                player_name: Some(player_name),
            })
        }
        nowhear::MediaEvent::StateChanged { player_name, .. } => {
            let player = source.get_player(&player_name).await.ok()?;
            let is_apple_music = player_name.to_lowercase().contains("applemusic");
            let Some(track) = player.current_track else {
                return Some(MediaInfo {
                    title: None,
                    album: None,
                    artist: None,
                    elapsed_time: None,
                    cover_artwork: None,
                    is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                    duration: None,
                    isrc: None,
                    player_name: Some(player_name),
                });
            };
            let artist = if is_apple_music {
                vec![track.artist[0].replace(" — ", " ")]
            } else {
                track.artist
            };
            Some(MediaInfo {
                title: Some(track.title),
                album: track
                    .album
                    .map(|album| sanitize_apple_music_album_name(&album)),
                artist: Some(artist.join(", ")),
                elapsed_time: player.position.map(|p| p.as_secs() as u32),
                cover_artwork: track
                    .artwork
                    .and_then(|art| CoverArtwork::from_nowhear_artwork(art)),
                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                duration: track.duration.map(|d| d.as_secs() as u32),
                isrc: None,
                player_name: Some(player_name),
            })
        }
        _ => None,
    }
}

fn sanitize_apple_music_album_name(album_name: &str) -> String {
    album_name
        .trim_end_matches(" - Single")
        .trim_end_matches(" - EP")
        .trim()
        .to_string()
}