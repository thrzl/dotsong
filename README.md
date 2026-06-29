<br />
<p align="center">
  <a href="https://github.com/thrzl/dotsong">
    <img src="src-tauri/icons/icon.png" alt="dotsong project logo" width="80">
  </a>

  <h3 align="center"><b>dotsong</b></h3>

  <p align="center">
    show off your music, freely
    <br />
    <!-- until i get the wiki together
        <a href="#"><strong>explore the docs »</strong></a> 
    <br />
    -->
    <!-- <br /> -->
    <!-- until i get a demo together too (i guess)
    <a href="https://github.com/thrzl/dotsong">view demo</a>
    ·-->
    <a href="https://github.com/thrzl/dotsong/issues">report bug</a>
    ·
    <a href="https://github.com/thrzl/dotsong/issues">request feature</a>

  </p>
</p>


dotsong is a simple cross-platform tray app that allows users to scrobble now playing tracks from anywhere to last.fm, listenbrainz, and libre.fm

it also allows you to display your current song as your discord presence

dotsong is currently alpha or something like that. should be stable and everything, it's just that it's very incomplete. see the todo list for more info on that

## installation note for macOS users:

i dont have a code signing license or whatever, so macOS will tell you that the app is damaged. you need to run the following in Terminal after you've installed:

```bash
xattr -c /Applications/dotsong.app
```

## why

nobody else does this for free, for some reason

### feats

- almost 0% CPU (the app spends most of its time waiting for updates)
- 30-40 MB memory (when the settings menu is closed)
- support for any listenbrainz/last.fm compatible scrobbling server
- completely free

## todo list

- [x] working settings menu
- [x] discord rich presence
- [x] listenbrainz scrobbling
- [x] last.fm scrobbling
- [x] libre.fm scrobbling (just last.fm to a diff url)
- [x] check for updates
- [x] cover art uploading

## goals

- be lightweight (at least lighter than music presence)
- be perfectable. this app should be written so that if i stopped updating it, 5 years later it'd still work great

## what's a dotsong

a reference to a [song](https://listenbrainz.org/track/361a0065-9eed-4ba2-be02-f87db26dadfc)

## how does it work

dotsong uses the media center on all platforms to access the currently playing song. on macOS, this is done via the private `MediaRemote` framework (which falls back to AppleScript, but that only really supports Apple Music and Spotify)