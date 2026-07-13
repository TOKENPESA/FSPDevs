# Generate FSPDevs-code-audit-v6.zip for Gemini deep audit upload.
# Excludes binaries, build artifacts, upstream submodule, and frozen snapshots.

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-code-audit-v6"
$zipPath = Join-Path $repo "FSPDevs-code-audit-v6.zip"

$excludeDirNames = @(
    "target", "node_modules", "pkg", ".git", "data",
    ".fiber-agent",
    "FSPDevs-code-audit-v4", "FSPDevs-code-audit-v5", "FSPDevs-code-audit-v6",
    "FSPDevs-gemini-audit-10", "FSPDevs-gemini-audit-11", "FSPDevs-gemini-audit-12",
    "FSPDevs-gemini-audit-13", "TPXDevs-gemini-audit-10", "TPXDevs-gemini-audit-11",
    "TPXDevs-gemini-audit-12", "TPXDevs-code-audit-v5",
    "gemini audit", "gemini audit2", "Fiber Readiness",
    "wasm", "demo", "fiber", "show-ckb-address"
)
$excludeFileNames = @(
    "fnn.exe", "fnn-cli.exe", "mesh-pubkeys.json",
    "FSPDevs-code-audit-v4.zip", "FSPDevs-code-audit-v5.zip", "FSPDevs-code-audit-v6.zip",
    "FSPDevs-gemini-audit-13.zip", "TPXDevs-gemini-audit-12.zip"
)
$excludeExtensions = @(".exe", ".pdb", ".dll", ".tar.gz", ".zip")

function Should-ExcludePath {
    param([string]$FullPath, [string]$RelativePath)

    $leaf = Split-Path $FullPath -Leaf
    if ($excludeFileNames -contains $leaf) { return $true }

    $ext = [System.IO.Path]::GetExtension($FullPath).ToLowerInvariant()
    if ($excludeExtensions -contains $ext) { return $true }
    if ($leaf -match '\.tar\.gz$') { return $true }

    $parts = $RelativePath -split '[\\/]'
    foreach ($part in $parts) {
        if ($excludeDirNames -contains $part) { return $true }
    }

    if ($RelativePath -match '\\data\\fiber\\') { return $true }
    if ($RelativePath -match '/data/fiber/') { return $true }

    return $false
}

function Copy-AuditTree {
    param(
        [string]$SourceRoot,
        [string]$DestRoot,
        [string[]]$RelativePaths
    )

    foreach ($rel in $RelativePaths) {
        $src = Join-Path $SourceRoot $rel
        if (-not (Test-Path $src)) {
            Write-Warning "Skip missing: $rel"
            continue
        }

        $dest = Join-Path $DestRoot $rel
        if ((Get-Item $src).PSIsContainer) {
            Get-ChildItem -Path $src -Recurse -File -Force | ForEach-Object {
                $fileRel = $_.FullName.Substring($src.Length).TrimStart('\', '/')
                $combinedRel = Join-Path $rel $fileRel
                if (Should-ExcludePath -FullPath $_.FullName -RelativePath $combinedRel) {
                    return
                }
                $target = Join-Path $DestRoot $combinedRel
                $targetDir = Split-Path $target -Parent
                if (-not (Test-Path $targetDir)) {
                    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
                }
                Copy-Item $_.FullName $target -Force
            }
        } else {
            if (-not (Should-ExcludePath -FullPath $src -RelativePath $rel)) {
                $targetDir = Split-Path $dest -Parent
                if (-not (Test-Path $targetDir)) {
                    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
                }
                Copy-Item $src $dest -Force
            }
        }
    }
}

Write-Host "=== FSPDevs Gemini Audit Bundle v6 ===" -ForegroundColor Cyan

if (Test-Path $outDir) {
    Remove-Item $outDir -Recurse -Force
}
New-Item -ItemType Directory -Path $outDir -Force | Out-Null

$paths = @(
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "jsconfig.json",
    "index.html",
    "mesh-core",
    "master-fiber-agent",
    "fiber-agent",
    "mesh-operator",
    "fsp-fixed-math",
    "packages",
    "dashboard",
    "mfa-console",
    "deploy",
    "fnn-testnet",
    "scripts",
    "GEMINI_DEEP_AUDIT_PROMPT.txt",
    "GEMINI_IMPLEMENTATION_STATUS.txt",
    "FIBER_ECOSYSTEM_BEST_IN_CLASS.txt",
    "FIBER_TESTNET_READINESS_REVIEW.txt",
    "SIMULATION_BASE.txt",
    "CODEBASE_MANIFEST.txt",
    "ARCHIVE_SNAPSHOTS.txt",
    "bundle-gemini-audit.ps1",
    "bundle-gemini-audit-13.ps1"
)

Copy-AuditTree -SourceRoot $repo -DestRoot $outDir -RelativePaths $paths

# Strip sidecar lockfile duplicates if workspace lock exists at root
$sidecarLock = Join-Path $outDir "fiber-agent\Cargo.lock"
if (Test-Path (Join-Path $outDir "Cargo.lock")) {
    if (Test-Path $sidecarLock) { Remove-Item $sidecarLock -Force }
}
$mfaLock = Join-Path $outDir "master-fiber-agent\Cargo.lock"
if (Test-Path $mfaLock) { Remove-Item $mfaLock -Force }
$operatorLock = Join-Path $outDir "mesh-operator\Cargo.lock"
if (Test-Path $operatorLock) { Remove-Item $operatorLock -Force }

# Count files for manifest sanity
$fileCount = (Get-ChildItem $outDir -Recurse -File).Count
Write-Host "Staged $fileCount files in $outDir"

if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath -CompressionLevel Optimal

$zipSizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
Write-Host ""
Write-Host "Created: $zipPath ($zipSizeMb MB)" -ForegroundColor Green
Write-Host ""
Write-Host "Upload to Gemini:" -ForegroundColor Yellow
Write-Host "  1. GEMINI_DEEP_AUDIT_PROMPT.txt (paste as prompt)"
Write-Host "  2. FSPDevs-code-audit-v6.zip (attach file)"
Write-Host "  Optional: FIBER_ECOSYSTEM_BEST_IN_CLASS.txt"
