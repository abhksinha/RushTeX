# Installs rushtex on Windows from its GitHub release, checked against the release's SHA256SUMS.
#
#   irm https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.ps1))) -Full -Version v0.1.0
#
# Options (or the environment variable beside each):
#   -Full             the full build, every font pack inside   (RUSHTEX_FULL=1)
#   -Version vX.Y.Z   that release instead of the latest       (RUSHTEX_VERSION)
#   -Dir DIR          where rushtex.exe goes, default %LOCALAPPDATA%\Programs\rushtex\bin (RUSHTEX_INSTALL_DIR)
# RUSHTEX_RELEASE_BASE replaces https://github.com/abhksinha/RushTeX/releases (a mirror, or a
# local folder laid out as latest/download/<asset> and download/<tag>/<asset>).
# No administrator rights: it writes only to the install folder and a temporary folder.
param(
    [switch]$Full,
    [string]$Version = $env:RUSHTEX_VERSION,
    [string]$Dir = $env:RUSHTEX_INSTALL_DIR
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Say($m) { Write-Host "rushtex install: $m" }

$base = if ($env:RUSHTEX_RELEASE_BASE) { $env:RUSHTEX_RELEASE_BASE.TrimEnd('/') } else { 'https://github.com/abhksinha/RushTeX/releases' }
if ($env:RUSHTEX_FULL -and $env:RUSHTEX_FULL -ne '0') { $Full = $true }
if (-not $Dir) { $Dir = Join-Path $env:LOCALAPPDATA 'Programs\rushtex\bin' }

# One Windows build, x86_64; Windows on Arm runs it under its x64 emulation.
$target = 'x86_64-pc-windows-msvc'
$name = "rushtex-$target" + $(if ($Full) { '-full' } else { '' })
$asset = "$name.zip"
$urlDir = if ($Version) { "$base/download/$Version" } else { "$base/latest/download" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("rushtex-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    # A plain path or file:// URL (a local test release) is copied; anything else downloaded.
    function Fetch($url, $out) {
        if ($url -match '^https?://') {
            [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out
        } else {
            Copy-Item -LiteralPath ($url -replace '^file:///?', '') -Destination $out
        }
    }
    $verLabel = if ($Version) { $Version } else { 'latest' }
    Say "downloading $asset ($verLabel)"
    try { Fetch "$urlDir/$asset" (Join-Path $tmp $asset) } catch { throw "could not download $urlDir/$asset" }
    try { Fetch "$urlDir/SHA256SUMS" (Join-Path $tmp 'SHA256SUMS') } catch { throw "could not download $urlDir/SHA256SUMS" }

    $want = $null
    foreach ($line in Get-Content (Join-Path $tmp 'SHA256SUMS')) {
        $f = $line -split '\s+', 2
        if ($f.Count -eq 2 -and ($f[1] -eq $asset -or $f[1] -eq "*$asset")) { $want = $f[0].ToLower(); break }
    }
    if (-not $want) { throw "SHA256SUMS names no $asset" }
    $got = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tmp $asset)).Hash.ToLower()
    if ($got -ne $want) { throw "checksum mismatch for $asset (expected $want, got $got); nothing installed" }
    Say 'checksum verified'

    Expand-Archive -LiteralPath (Join-Path $tmp $asset) -DestinationPath $tmp
    $exe = Join-Path $tmp "$name\rushtex.exe"
    if (-not (Test-Path -LiteralPath $exe)) { throw "$asset holds no $name\rushtex.exe" }
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $Dir 'rushtex.exe') -Force
    Say "installed $(Join-Path $Dir 'rushtex.exe')"
} catch {
    # Not `exit`: under `irm | iex` it would close the user's PowerShell window.
    Write-Host "rushtex install: error: $($_.Exception.Message)" -ForegroundColor Red
    $global:LASTEXITCODE = 1
    return
} finally {
    Remove-Item -Recurse -Force -LiteralPath $tmp -ErrorAction SilentlyContinue
}

$onPath = ($env:Path -split ';') | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') }
if ($onPath) {
    Say 'run: rushtex paper.tex'
} else {
    Say "$Dir is not on your PATH. Add it for your user, then open a new terminal:"
    Write-Host "    [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$Dir', 'User')"
}
