#!/bin/sh
set -eu
V2="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
QA_ROOT="$V2/.qa"
mkdir -p "$QA_ROOT/home" "$QA_ROOT/tmp" "$QA_ROOT/cache" "$QA_ROOT/config" "$QA_ROOT/data" "$QA_ROOT/cargo" "$QA_ROOT/npm"
export HOME="$QA_ROOT/home" TMPDIR="$QA_ROOT/tmp" TMP="$QA_ROOT/tmp" TEMP="$QA_ROOT/tmp"
export XDG_CACHE_HOME="$QA_ROOT/cache" XDG_CONFIG_HOME="$QA_ROOT/config" XDG_DATA_HOME="$QA_ROOT/data"
export CARGO_HOME="$QA_ROOT/cargo" CARGO_TARGET_DIR="$QA_ROOT/target" npm_config_cache="$QA_ROOT/npm"
export TZ=UTC
export PYTHONDONTWRITEBYTECODE=1 CARGO_NET_GIT_FETCH_WITH_CLI=false GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
export RUSTUP_HOME=/Users/almat/.rustup RUSTUP_AUTO_INSTALL=0
export PATH="/Users/almat/.rustup/toolchains/${QA_TOOLCHAIN:-stable}-aarch64-apple-darwin/bin:$PATH"
cd "$V2"
exec "$@"
