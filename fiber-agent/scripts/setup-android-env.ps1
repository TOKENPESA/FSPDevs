# Fiber Agent — Tauri 2 Android environment bootstrap (Windows)
#
# Step 4A: Prepare the Environment
# 1) Install Android Studio
# 2) SDK Manager → install:
#      - Android SDK Platform 34+ (or latest stable)
#      - Android SDK Build-Tools
#      - NDK (Side by side)
#      - Android SDK Command-line Tools
# 3) Run this script in the current PowerShell session (or permanently with -Persist)
#
# Usage:
#   . .\scripts\setup-android-env.ps1
#   . .\scripts\setup-android-env.ps1 -Persist
#   . .\scripts\setup-android-env.ps1 -SdkRoot "D:\Android\Sdk"

[CmdletBinding()]
param(
  [string]$SdkRoot = "",
  [switch]$Persist,
  [switch]$InstallRustTargets
)

$ErrorActionPreference = "Stop"

function Resolve-AndroidSdkRoot {
  param([string]$Preferred)
  if ($Preferred -and (Test-Path $Preferred)) {
    return (Resolve-Path $Preferred).Path
  }
  if ($env:ANDROID_HOME -and (Test-Path $env:ANDROID_HOME)) {
    return $env:ANDROID_HOME
  }
  if ($env:ANDROID_SDK_ROOT -and (Test-Path $env:ANDROID_SDK_ROOT)) {
    return $env:ANDROID_SDK_ROOT
  }
  $candidates = @(
    (Join-Path $env:LOCALAPPDATA "Android\Sdk"),
    "C:\Android\Sdk"
  )
  foreach ($c in $candidates) {
    if (Test-Path $c) { return $c }
  }
  return $null
}

function Resolve-NdkHome {
  param([string]$AndroidHome)
  if ($env:NDK_HOME -and (Test-Path $env:NDK_HOME)) {
    return $env:NDK_HOME
  }
  if ($env:ANDROID_NDK_HOME -and (Test-Path $env:ANDROID_NDK_HOME)) {
    return $env:ANDROID_NDK_HOME
  }
  $ndkRoot = Join-Path $AndroidHome "ndk"
  if (-not (Test-Path $ndkRoot)) {
    return $null
  }
  # Prefer latest installed side-by-side NDK (avoid colorized ls pitfalls from bash guides).
  $latest = Get-ChildItem $ndkRoot -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    Select-Object -First 1
  if ($null -eq $latest) { return $null }
  return $latest.FullName
}

function Resolve-JavaHome {
  if ($env:JAVA_HOME -and (Test-Path $env:JAVA_HOME)) {
    return $env:JAVA_HOME
  }
  $studioJbr = @(
    "${env:ProgramFiles}\Android\Android Studio\jbr",
    "${env:ProgramFiles}\Android\Android Studio\jre",
    "${env:LOCALAPPDATA}\Programs\Android Studio\jbr"
  )
  foreach ($j in $studioJbr) {
    if (Test-Path $j) { return $j }
  }
  return $null
}

function Set-EnvVarSession {
  param([string]$Name, [string]$Value)
  Set-Item -Path "Env:$Name" -Value $Value
  Write-Host "  $Name=$Value"
}

function Set-EnvVarPersistent {
  param([string]$Name, [string]$Value)
  [Environment]::SetEnvironmentVariable($Name, $Value, "User")
}

$androidHome = Resolve-AndroidSdkRoot -Preferred $SdkRoot
if (-not $androidHome) {
  Write-Host ""
  Write-Host "Android SDK not found." -ForegroundColor Yellow
  Write-Host "Install Android Studio, open SDK Manager, and install:"
  Write-Host "  - Android SDK Platform (34+)"
  Write-Host "  - Android SDK Build-Tools"
  Write-Host "  - NDK (Side by side)"
  Write-Host "  - Android SDK Command-line Tools"
  Write-Host ""
  Write-Host "Default Windows SDK path:"
  Write-Host "  $env:LOCALAPPDATA\Android\Sdk"
  Write-Host ""
  Write-Host "Then re-run:"
  Write-Host "  . .\scripts\setup-android-env.ps1"
  exit 1
}

$ndkHome = Resolve-NdkHome -AndroidHome $androidHome
if (-not $ndkHome) {
  Write-Host ""
  Write-Host "NDK (Side by side) not found under $androidHome\ndk" -ForegroundColor Yellow
  Write-Host "In Android Studio -> SDK Manager -> SDK Tools -> enable 'NDK (Side by side)'."
  exit 1
}

$javaHome = Resolve-JavaHome

Write-Host "Applying Android env for this session:" -ForegroundColor Cyan
Set-EnvVarSession -Name "ANDROID_HOME" -Value $androidHome
Set-EnvVarSession -Name "ANDROID_SDK_ROOT" -Value $androidHome
Set-EnvVarSession -Name "NDK_HOME" -Value $ndkHome
Set-EnvVarSession -Name "ANDROID_NDK_HOME" -Value $ndkHome
if ($javaHome) {
  Set-EnvVarSession -Name "JAVA_HOME" -Value $javaHome
} else {
  Write-Host "  JAVA_HOME not detected - Android Studio JBR recommended." -ForegroundColor Yellow
}

# Command-line tools + platform-tools on PATH for this session
$binPaths = @(
  (Join-Path $androidHome "platform-tools"),
  (Join-Path $androidHome "cmdline-tools\latest\bin"),
  (Join-Path $androidHome "emulator")
) | Where-Object { Test-Path $_ }
if ($binPaths.Count -gt 0) {
  $env:Path = ($binPaths -join ";") + ";" + $env:Path
  Write-Host '  PATH += platform-tools / cmdline-tools / emulator'
}

# Lock mobile builds to TLS MFA + live FNN testnet (Android blocks cleartext by default).
if (-not $env:MFA_HOST) {
  Set-EnvVarSession -Name "MFA_HOST" -Value "mfa.fsprotocol.com"
}
if (-not $env:MFA_WS_SECURE) {
  Set-EnvVarSession -Name "MFA_WS_SECURE" -Value "true"
}
if (-not $env:FNN_MODE) {
  Set-EnvVarSession -Name "FNN_MODE" -Value "testnet"
}

if ($Persist) {
  Write-Host ""
  Write-Host "Persisting User environment variables..." -ForegroundColor Cyan
  Set-EnvVarPersistent -Name "ANDROID_HOME" -Value $androidHome
  Set-EnvVarPersistent -Name "ANDROID_SDK_ROOT" -Value $androidHome
  Set-EnvVarPersistent -Name "NDK_HOME" -Value $ndkHome
  Set-EnvVarPersistent -Name "ANDROID_NDK_HOME" -Value $ndkHome
  if ($javaHome) {
    Set-EnvVarPersistent -Name "JAVA_HOME" -Value $javaHome
  }
  Write-Host "Re-open terminals (or sign out/in) so other apps pick up the User env."
}

if ($InstallRustTargets) {
  Write-Host ""
  Write-Host "Installing Rust Android targets..." -ForegroundColor Cyan
  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
}

Write-Host ""
Write-Host "Next steps (from fiber-agent/):" -ForegroundColor Green
Write-Host "  npm install"
Write-Host "  npm run tauri android init"
Write-Host "  npm run tauri:android:dev     # emulator / device"
Write-Host "  npm run tauri:android:build   # APK / AAB"
Write-Host ""
