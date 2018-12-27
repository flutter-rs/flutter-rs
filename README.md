# flutter-rs [![Join Gitter Chat Channel](https://badges.gitter.im/flutter-rs/community.svg)](https://gitter.im/flutter-rs/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

**Build flutter desktop app in dart & rust**

This is the development repo. Head to [flutter-app-template](https://github.com/gliheng/flutter-app-template) for a running demo.

# Install
- Install latest [Rust](https://www.rust-lang.org)
- Install libglfw:
    - Mac: `brew install glfw`
    - linux: `apt install libglfw3`
- Run `cargo run` to get a running example.
    Note: The first run is going to take a while to download rust dependecies and flutter engine.


# Features:
- Support Hot reload
- MethodChannle (Only support JsonMethodChannel now)
- Application icons
- Clipboard support
- Cross platform support (mac & linux)
- Support distribution format: (only mac app now)

# Roadmap:

## 0.2
- Support setting default window background color.
- Loader UI and rebranding.
- Desktop integration: App menu, context menu, file dialogs.
- Flutter scroller should support desktop scroll event.
- Download dll from web?

# Contribution
Contribution and PR are welcome.