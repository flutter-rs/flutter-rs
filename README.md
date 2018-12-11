This project uses libflutter_engine to implement a desktop flutter runner.
[This guide](https://github.com/google/flutter-desktop-embedding)  describes how it works.

# Install
- install latest [Rust](https://www.rust-lang.org)
- Run `cargo run` to get a running example.
    Note: The first run is going to take a while to download rust dependecies and flutter engine.


# Features:

# Roadmap:

## 0.1
- Cross platform support.
- Clipboard support.
- Support distribution format. That is: app package for mac. Linux should support snap or deb, Windows support windows installer.
- Export flutter rust lib on crates.io
- Support setting default window background color.
- Application icons.

## 0.2
- Support Hot reload.
- Loader UI and rebranding.
- Desktop integration: App menu, context menu, file dialogs.
- Flutter scroller should support desktop scroll event.
- Download dll from web