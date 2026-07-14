#!/usr/bin/env bash
# Fiber Agent — Tauri 2 Android environment bootstrap (macOS / Linux)
# Step 4A counterpart to scripts/setup-android-env.ps1
set -euo pipefail

# macOS default; override with ANDROID_HOME if already set.
if [[ -z "${ANDROID_HOME:-}" ]]; then
  if [[ -d "${HOME}/Library/Android/sdk" ]]; then
    export ANDROID_HOME="${HOME}/Library/Android/sdk"
  elif [[ -d "${HOME}/Android/Sdk" ]]; then
    export ANDROID_HOME="${HOME}/Android/Sdk"
  fi
fi

if [[ -z "${ANDROID_HOME:-}" || ! -d "${ANDROID_HOME}" ]]; then
  echo "ANDROID_HOME not found. Install Android Studio + SDK, then re-run."
  echo "  macOS:  \$HOME/Library/Android/sdk"
  echo "  Linux:  \$HOME/Android/Sdk"
  exit 1
fi

export ANDROID_SDK_ROOT="${ANDROID_HOME}"

# Pick latest side-by-side NDK without relying on colorized `ls`.
if [[ -z "${NDK_HOME:-}" ]]; then
  NDK_DIR="$(find "${ANDROID_HOME}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1 || true)"
  if [[ -z "${NDK_DIR}" ]]; then
    echo "NDK (Side by side) missing under ${ANDROID_HOME}/ndk"
    echo "Android Studio → SDK Manager → SDK Tools → enable NDK (Side by side)."
    exit 1
  fi
  export NDK_HOME="${NDK_DIR}"
fi
export ANDROID_NDK_HOME="${NDK_HOME}"

export MFA_HOST="${MFA_HOST:-mfa.fsprotocol.com}"
export MFA_WS_SECURE="${MFA_WS_SECURE:-true}"
export FNN_MODE="${FNN_MODE:-testnet}"

echo "ANDROID_HOME=${ANDROID_HOME}"
echo "NDK_HOME=${NDK_HOME}"
echo "MFA_HOST=${MFA_HOST} MFA_WS_SECURE=${MFA_WS_SECURE} FNN_MODE=${FNN_MODE}"
echo
echo "Next:"
echo "  cd fiber-agent && npm run tauri android init"
echo "  npm run tauri:android:dev"
echo "  npm run tauri:android:build"
