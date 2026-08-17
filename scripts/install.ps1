$ErrorActionPreference = 'Stop'

$repository = 'memorax-ai/dsh-patchouli'
$version = if ($env:PATCHOULI_VERSION) { $env:PATCHOULI_VERSION } else { 'latest' }
$installDir = if ($env:PATCHOULI_INSTALL_DIR) {
    [IO.Path]::GetFullPath($env:PATCHOULI_INSTALL_DIR)
} else {
    Join-Path $env:LOCALAPPDATA 'Patchouli\bin'
}
$patchouliHome = if ($env:PATCHOULI_HOME) {
    [IO.Path]::GetFullPath($env:PATCHOULI_HOME)
} else {
    Join-Path $HOME '.patchouli'
}

if ($env:PROCESSOR_ARCHITECTURE -notin @('AMD64', 'x86_64')) {
    throw "unsupported Windows architecture: $($env:PROCESSOR_ARCHITECTURE)"
}
$asset = 'patchouli-db-windows-x86_64.exe'
$releaseUrl = if ($version -eq 'latest') {
    "https://github.com/$repository/releases/latest/download"
} else {
    "https://github.com/$repository/releases/download/$version"
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
$temporaryDir = Join-Path ([IO.Path]::GetTempPath()) "patchouli-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $temporaryDir | Out-Null
$downloadedBinary = Join-Path $temporaryDir $asset
$downloadedChecksum = "$downloadedBinary.sha256"
$stagedBinary = Join-Path $installDir ".patchouli-$([guid]::NewGuid()).exe"
$backupBinary = Join-Path $installDir ".patchouli-backup-$([guid]::NewGuid()).exe"
$target = Join-Path $installDir 'patchouli-db.exe'

try {
    Invoke-WebRequest -Uri "$releaseUrl/$asset" -OutFile $downloadedBinary
    Invoke-WebRequest -Uri "$releaseUrl/$asset.sha256" -OutFile $downloadedChecksum
    $expected = ((Get-Content -LiteralPath $downloadedChecksum -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -LiteralPath $downloadedBinary -Algorithm SHA256).Hash
    if ($actual -ne $expected) {
        throw "checksum mismatch for $asset"
    }

    Copy-Item -LiteralPath $downloadedBinary -Destination $stagedBinary
    & $stagedBinary init --root $patchouliHome
    if ($LASTEXITCODE -ne 0) {
        throw "staged patchouli-db init failed with exit code $LASTEXITCODE"
    }
    if ([IO.File]::Exists($target)) {
        try {
            [IO.File]::Replace($stagedBinary, $target, $backupBinary)
        } catch {
            if (-not [IO.File]::Exists($target) -and [IO.File]::Exists($backupBinary)) {
                [IO.File]::Move($backupBinary, $target)
            }
            throw
        }
        Remove-Item -LiteralPath $backupBinary -Force
    } else {
        [IO.File]::Move($stagedBinary, $target)
    }
    Write-Host "installed Patchouli DB to $target"
    if (($env:PATH -split ';') -notcontains $installDir) {
        Write-Host "add $installDir to PATH to invoke patchouli-db directly"
    }
} finally {
    Remove-Item -LiteralPath $downloadedBinary, $downloadedChecksum -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stagedBinary -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temporaryDir -Force -ErrorAction SilentlyContinue
}
