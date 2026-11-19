#!/usr/bin/env bash
# Build the example's Dart assets and run them on flutter-glfw.
#
# The engine is the generic embedder for the SDK's engine revision. Set
# FLUTTER_ENGINE_LIB_DIR / LD_LIBRARY_PATH if you already have it; otherwise
# this script downloads it into .engine.
set -euo pipefail
cd "$(dirname "$0")"

FLUTTER_ROOT="${FLUTTER_ROOT:-$(dirname "$(dirname "$(command -v flutter)")")}"
ENGINE_HASH="$(cat "$FLUTTER_ROOT/bin/cache/engine.stamp" 2>/dev/null \
  || cat "$FLUTTER_ROOT/bin/internal/engine.version")"
ENGINE_DIR="${FLUTTER_ENGINE_LIB_DIR:-$PWD/.engine}"

if [[ ! -f "$ENGINE_DIR/libflutter_engine.so" ]]; then
  echo "fetching the generic embedder for engine $ENGINE_HASH into $ENGINE_DIR"
  mkdir -p "$ENGINE_DIR"
  curl -fL -o "$ENGINE_DIR/embedder.zip" \
    "https://storage.googleapis.com/flutter_infra_release/flutter/$ENGINE_HASH/linux-x64/linux-x64-embedder.zip"
  unzip -o "$ENGINE_DIR/embedder.zip" -d "$ENGINE_DIR"
  rm -f "$ENGINE_DIR/embedder.zip"
fi

flutter pub get
flutter build bundle -t lib/main_glfw.dart

FLUTTER_ENGINE_LIB_DIR="$ENGINE_DIR" \
  cargo build --manifest-path runner/Cargo.toml

ICU="${FLUTTER_ICU_DATA:-$FLUTTER_ROOT/bin/cache/artifacts/engine/linux-x64/icudtl.dat}"
FLUTTER_ASSET_DIR="${FLUTTER_ASSET_DIR:-$PWD/build/flutter_assets}" \
  FLUTTER_ICU_DATA="$ICU" \
  LD_LIBRARY_PATH="$ENGINE_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  exec ./runner/target/debug/multiple-windows-runner
