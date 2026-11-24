#!/usr/bin/env bash
# Build the example's Dart assets and run them on flutter-glfw (macOS).
#
# The macOS engine ships without an OpenGL backend, so the app runs on the
# engine's software renderer: flutter-glfw rasterises each view on the CPU and
# the compositor uploads the pixels through the view's OpenGL context.
set -euo pipefail
cd "$(dirname "$0")"

FLUTTER_ROOT="${FLUTTER_ROOT:-$(dirname "$(dirname "$(command -v flutter)")")}"
FLUTTER="$FLUTTER_ROOT/bin/flutter"
# `engine.stamp` holds the engine revision, which storage.googleapis.com also
# accepts in place of the content hash.
ENGINE_HASH="$(cat "$FLUTTER_ROOT/bin/cache/engine.stamp" 2>/dev/null \
  || cat "$FLUTTER_ROOT/bin/internal/engine.version")"
ENGINE_DIR="${FLUTTER_ENGINE_LIB_DIR:-$PWD/.engine}"

# glfw-sys builds GLFW with CMake, and Homebrew's bin directory is not on the
# PATH of a non-interactive shell.
export PATH="/opt/homebrew/bin:$PATH"

if [[ ! -d "$ENGINE_DIR/FlutterEmbedder.framework" ]]; then
  echo "fetching the generic macOS embedder for engine $ENGINE_HASH into $ENGINE_DIR"
  mkdir -p "$ENGINE_DIR/FlutterEmbedder.framework"
  curl -fL -o "$ENGINE_DIR/embedder.zip" \
    "https://storage.googleapis.com/flutter_infra_release/flutter/$ENGINE_HASH/darwin-x64/FlutterEmbedder.framework.zip"
  unzip -oq "$ENGINE_DIR/embedder.zip" -d "$ENGINE_DIR/FlutterEmbedder.framework"
  rm -f "$ENGINE_DIR/embedder.zip"
fi

"$FLUTTER" pub get
"$FLUTTER" build bundle -t lib/main_glfw.dart

FLUTTER_ENGINE_LIB_DIR="$ENGINE_DIR" cargo build --manifest-path runner/Cargo.toml

ICU="${FLUTTER_ICU_DATA:-$FLUTTER_ROOT/bin/cache/artifacts/engine/darwin-x64/icudtl.dat}"
# The framework rpath recorded by flutter-engine-sys only covers that crate's
# own targets; a downstream binary still needs the framework directory here.
DYLD_FRAMEWORK_PATH="$ENGINE_DIR" \
  FLUTTER_ASSET_DIR="${FLUTTER_ASSET_DIR:-$PWD/build/flutter_assets}" \
  FLUTTER_ICU_DATA="$ICU" \
  exec ./runner/target/debug/multiple-windows-runner
