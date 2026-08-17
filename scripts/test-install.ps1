$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testBinary = Join-Path $repositoryRoot 'target\debug\patchouli-db.exe'
$sandbox = Join-Path ([IO.Path]::GetTempPath()) "patchouli-installer-test-$([guid]::NewGuid())"
$installDir = Join-Path $sandbox 'bin'
$patchouliHome = Join-Path $sandbox 'home'
$target = Join-Path $installDir 'patchouli-db.exe'

function Invoke-WebRequest {
    param([string]$Uri, [string]$OutFile)

    if ($Uri.EndsWith('.sha256')) {
        $hash = (Get-FileHash -LiteralPath $testBinary -Algorithm SHA256).Hash
        Set-Content -LiteralPath $OutFile -Value "$hash  patchouli-db-windows-x86_64.exe"
    } else {
        Copy-Item -LiteralPath $testBinary -Destination $OutFile
    }
}

try {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    & $testBinary init --root $patchouliHome
    if ($LASTEXITCODE -ne 0) { throw 'test setup init failed' }
    Set-Content -LiteralPath (Join-Path $patchouliHome 'config.json') -Value '{"invalid":true}'
    Set-Content -LiteralPath $target -Value 'existing installation'

    $env:PATCHOULI_INSTALL_DIR = $installDir
    $env:PATCHOULI_HOME = $patchouliHome
    $env:PATCHOULI_VERSION = 'test'
    $failed = $false
    try {
        . (Join-Path $PSScriptRoot 'install.ps1')
    } catch {
        $failed = $true
    }

    if (-not $failed) { throw 'installer accepted a failed staged init' }
    if ((Get-Content -LiteralPath $target -Raw).Trim() -ne 'existing installation') {
        throw 'installer replaced the existing binary after staged init failed'
    }
} finally {
    Remove-Item Env:PATCHOULI_INSTALL_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:PATCHOULI_HOME -ErrorAction SilentlyContinue
    Remove-Item Env:PATCHOULI_VERSION -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}

$global:LASTEXITCODE = 0
