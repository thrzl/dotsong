# dotsong <img src="src-tauri/icons/tray-icon.png" height="22"> 

dotsong is a simple cross-platform tray app that allows users to scrobble now playing tracks from anywhere to last.fm, listenbrainz, and libre.fm

it also allows you to display your current song as your discord presence

## why

nobody else does this for free, for some reason

## todo list

- [x] discord rich presence
- [ ] listenbrainz scrobbling
- [ ] last.fm scrobbling
- [ ] libre.fm scrobbling (just last.fm to a diff url)

## goals

- be lightweight (at least lighter than music presence)
- be perfectable. this app should be written so that if i stopped updating it, 5 years later it'd still work great

## what's a dotsong

a reference to a [song](https://listenbrainz.org/track/361a0065-9eed-4ba2-be02-f87db26dadfc)

## how does it work

dotsong uses the media center on all platforms to access the currently playing song. on macOS, this is done via the private `MediaRemote` framework (which falls back to AppleScript, but that only really supports Apple Music and Spotify)