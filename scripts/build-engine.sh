#!/usr/bin/env bash
#
# build-engine.sh — build libflutter_engine.so matching the installed Flutter SDK.
#
# The public Flutter artifact storage only ships a JIT (debug) build of the
# generic embedder (linux-x64-embedder.zip). To run an app in AOT/release you
# need a release-mode libflutter_engine.so, which Google does not distribute —
# it has to be built from the engine source at the *exact same commit* as your
# SDK so the Dart VM matches the SDK's gen_snapshot.
#
# This script detects the installed SDK, checks out flutter/flutter at the
# matching version, builds the embedder library in the requested runtime mode,
# and copies it where you ask.
#
# Since the 2024 monorepo merge the engine lives inside flutter/flutter at
# engine/src/flutter, bootstrapped by engine/scripts/standard.gclient and built
# with engine/src/flutter/tools/gn. That layout, and every gn flag used below,
# was last checked against the 3.44.2 and 3.47.8 tags.
#
# Expect the first run to take hours and tens of GB: `gclient sync` fetches the
# whole engine dependency tree before anything is compiled.
#
# Usage:
#   scripts/build-engine.sh [--mode release|profile|debug] [--out DIR] [--jobs N]
#   scripts/build-engine.sh --help
#
# Environment overrides:
#   FLUTTER_ROOT   Flutter SDK path (default: derived from `which flutter`)
#   BUILD_ROOT     where the engine source/checkout lives (default: ~/flutter-engine-build)
#   MODE           runtime mode (default: release)
#   OUT_DIR        copy the .so here (default: none — just print the path)
#   JOBS           parallelism (default: nproc)
#
# Examples:
#   # Build a release engine and drop it into the app template:
#   scripts/build-engine.sh --out ~/Work/flutter-app-template/.engine-release
#
#   # Build a debug (JIT) engine instead:
#   scripts/build-engine.sh --mode debug --out ./.engine
#
set -euo pipefail

MODE="${MODE:-release}"
BUILD_ROOT="${BUILD_ROOT:-$HOME/flutter-engine-build}"
OUT_DIR="${OUT_DIR:-}"
JOBS="${JOBS:-$(nproc)}"

# Print this file's header comment (everything from the line after the shebang
# up to the first non-comment line), with the leading '# ' stripped.
usage() { awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"; }

# Usage errors exit 2 and are reported before the log helpers exist.
usage_error() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 2; }

# ---- parse flags ----------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    # `${2:-}` rather than `$2`: under `set -u` a flag given without a value
    # would otherwise abort with bash's own "unbound variable" message instead
    # of the one below.
    --mode) MODE="${2:-}"; [[ -n "$MODE" ]]    || usage_error "--mode needs a value"; shift 2;;
    --out)  OUT_DIR="${2:-}"; [[ -n "$OUT_DIR" ]] || usage_error "--out needs a value"; shift 2;;
    --jobs) JOBS="${2:-}"; [[ -n "$JOBS" ]]    || usage_error "--jobs needs a value"; shift 2;;
    -h|--help) usage; exit 0;;
    *) usage_error "unknown argument: $1 (try --help)";;
  esac
done

case "$MODE" in
  release) OUTSUB=host_release;;
  profile) OUTSUB=host_profile;;
  debug)   OUTSUB=host_debug;;
  *) usage_error "invalid --mode '$MODE' (use release|profile|debug)";;
esac

# Both `gclient sync` and `ninja` take this straight through to -j.
[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || usage_error "invalid --jobs '$JOBS' (expected a positive integer)"

log() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ---- 1. detect the installed Flutter SDK ----------------------------------
if [[ -z "${FLUTTER_ROOT:-}" ]]; then
  command -v flutter >/dev/null || die "flutter not found in PATH (set FLUTTER_ROOT)"
  FLUTTER_ROOT="$(cd "$(dirname "$(dirname "$(readlink -f "$(command -v flutter)")")")" && pwd)"
fi
[[ -d "$FLUTTER_ROOT" ]] || die "FLUTTER_ROOT does not exist: $FLUTTER_ROOT"

FRAMEWORK_SHA="$(git -C "$FLUTTER_ROOT" rev-parse HEAD 2>/dev/null || true)"
# `$FLUTTER_ROOT/version` was removed from the SDK years ago; the framework
# version now lives in bin/cache/flutter.version.json (written the first time
# `flutter` runs), with the git tag as a fallback for a fresh checkout.
FLUTTER_VERSION="$(
  sed -n 's/.*"frameworkVersion"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    "$FLUTTER_ROOT/bin/cache/flutter.version.json" 2>/dev/null | head -1 || true
)"
if [[ -z "$FLUTTER_VERSION" ]]; then
  FLUTTER_VERSION="$(git -C "$FLUTTER_ROOT" describe --tags --exact-match 2>/dev/null || true)"
fi

# bin/cache/engine.stamp is what the flutter tool itself reads and exists on
# every channel; bin/internal/engine.version is only committed on a tagged
# release. Logged only — the build uses the framework checkout, not this hash.
ENGINE_VERSION="$(
  {
    tr -d '[:space:]' < "$FLUTTER_ROOT/bin/cache/engine.stamp" ||
      tr -d '[:space:]' < "$FLUTTER_ROOT/bin/internal/engine.version"
  } 2>/dev/null || true
)"

log "Flutter SDK:       $FLUTTER_ROOT"
log "Flutter version:   ${FLUTTER_VERSION:-unknown}"
log "Framework commit:  ${FRAMEWORK_SHA:-unknown}"
log "Engine version:    ${ENGINE_VERSION:-unknown}"
log "Build mode:        $MODE  (out/$OUTSUB)"

[[ -n "$FRAMEWORK_SHA$FLUTTER_VERSION" ]] || die "could not determine the SDK's version/commit"

# ---- 2. depot_tools --------------------------------------------------------
mkdir -p "$BUILD_ROOT"
DEPOT="$BUILD_ROOT/depot_tools"
if [[ ! -d "$DEPOT" ]]; then
  log "Cloning depot_tools..."
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git "$DEPOT"
fi
export PATH="$DEPOT:$PATH"
export DEPOT_TOOLS_UPDATE="${DEPOT_TOOLS_UPDATE:-1}"

# ---- 3. check out flutter/flutter at the matching version ------------------
SRC_ROOT="$BUILD_ROOT/flutter"
mkdir -p "$SRC_ROOT"
cd "$SRC_ROOT"
git init -q
git remote add origin https://github.com/flutter/flutter.git 2>/dev/null \
  || git remote set-url origin https://github.com/flutter/flutter.git

current="$(git rev-parse HEAD 2>/dev/null || true)"
if [[ -n "$FRAMEWORK_SHA" && "$current" == "$FRAMEWORK_SHA" ]]; then
  log "Source already at $FRAMEWORK_SHA — skipping fetch."
else
  log "Fetching flutter source (${FLUTTER_VERSION:-$FRAMEWORK_SHA})..."
  if [[ -n "$FRAMEWORK_SHA" ]] && git fetch --depth 1 origin "$FRAMEWORK_SHA" 2>/dev/null; then
    git checkout -q FETCH_HEAD
  elif [[ -n "$FLUTTER_VERSION" ]] && \
       git fetch --depth 1 origin "refs/tags/$FLUTTER_VERSION:refs/tags/$FLUTTER_VERSION" 2>/dev/null; then
    git checkout -q "$FLUTTER_VERSION"
  else
    die "could not fetch flutter source at $FRAMEWORK_SHA / $FLUTTER_VERSION"
  fi
fi

# ---- 4. gclient sync (engine deps) ----------------------------------------
cp "$SRC_ROOT/engine/scripts/standard.gclient" "$SRC_ROOT/.gclient"
log "Running gclient sync (this downloads the engine deps; first run is slow)..."
gclient sync --no-history --shallow -j"$JOBS"

ENGINE_SRC="$SRC_ROOT/engine/src"
[[ -x "$ENGINE_SRC/flutter/tools/gn" ]] || die "engine source not populated (missing flutter/tools/gn)"

# ---- 5. configure + build the embedder library ----------------------------
cd "$ENGINE_SRC"
log "Configuring ($MODE)..."
flutter/tools/gn --runtime-mode="$MODE" --no-rbe --no-lto --prebuilt-dart-sdk

log "Building flutter_engine_library with $JOBS jobs..."
ninja -C "out/$OUTSUB" -j"$JOBS" flutter_engine_library

SO="$ENGINE_SRC/out/$OUTSUB/libflutter_engine.so"
[[ -f "$SO" ]] || die "build finished but $SO is missing"
log "Built: $SO ($(du -h "$SO" | cut -f1))"

# ---- 6. install --------------------------------------------------------------
if [[ -n "$OUT_DIR" ]]; then
  mkdir -p "$OUT_DIR"
  cp -f "$SO" "$OUT_DIR/"
  log "Copied to: $OUT_DIR/libflutter_engine.so"
  # icudtl.dat is shipped separately by the SDK and is needed at runtime.
  ICU="$(find "$FLUTTER_ROOT/bin/cache/artifacts/engine" -name icudtl.dat 2>/dev/null | head -1 || true)"
  # There is no FLUTTER_ICU_DATA environment variable: the only way to feed
  # this path in is the builder API.
  [[ -n "$ICU" ]] &&
    log "icudtl.dat (pass to FlutterEngineBuilder::with_icu_data_path): $ICU"
else
  log "No --out given. Use it directly, e.g.:"
  echo "    LD_LIBRARY_PATH=$(dirname "$SO") ..."
fi

log "Done."
