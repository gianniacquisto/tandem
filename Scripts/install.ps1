# The standalone Windows install: `irm <release>/install.ps1 | iex`.
#
# The PowerShell twin of install.sh. A tag publishes one archive per platform,
# `install.ps1` and `SHA256SUMS`; this script downloads the Windows zip,
# verifies its digest from the release's own manifest before extracting
# anything, installs the one `tandem.exe` into the user's profile and adds that
# folder to the *user* PATH when it is absent. It needs no elevation, never
# touches the machine PATH and never writes outside the install directory, the
# user PATH value and a temp directory it removes. Windows PowerShell 5.1 (stock
# Windows) is the floor.
#
# Usage: install.ps1 [-Version <1.2.3|v1.2.3|latest>] [-InstallDir <dir>] [-Reinstall]
#   -Version X     install tag vX (default: $env:TANDEM_VERSION, else the staged tag)
#   -InstallDir D  install into D (default: $env:TANDEM_INSTALL_DIR, else
#                  %LOCALAPPDATA%\Programs\tandem)
#   -Reinstall     replace an existing tandem.exe in the install directory
# `irm ... | iex` cannot pass arguments, so set $env:TANDEM_VERSION first to pin
# a version. Stop `tandem serve` before -Reinstall: Windows will not overwrite a
# running executable.
#
# Test seams (Scripts/test_install.ps1 drives these; production never sets them):
# TANDEM_BASE_URL (a local directory holding SHA256SUMS and the zip, flat, in
# place of the release URL) and TANDEM_FAKE_USER_PATH_FILE (a file standing in
# for the user PATH value, so a test never edits the runner's real PATH).
param(
  [string]$Version = '',
  [string]$InstallDir = '',
  [switch]$Reinstall
)

# Stamped by Scripts/stage-release-assets.sh at release time. In the repo this
# reads "latest"; a staged copy names its tag's version.
$DefaultVersion = "latest"

function Install-Tandem {
  param([string]$Version, [string]$InstallDir, [bool]$Reinstall)

  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  # Windows PowerShell 5.1 negotiates TLS 1.0 by default; GitHub needs 1.2.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $baseUrl = if ($env:TANDEM_BASE_URL) { $env:TANDEM_BASE_URL } else { 'https://github.com/gianniacquisto/tandem/releases' }
  $localSource = Test-Path -LiteralPath $baseUrl -PathType Container

  if (-not $Version) {
    $Version = if ($env:TANDEM_VERSION) { $env:TANDEM_VERSION } else { $DefaultVersion }
  }
  if (-not $InstallDir) {
    if ($env:TANDEM_INSTALL_DIR) {
      $InstallDir = $env:TANDEM_INSTALL_DIR
    } else {
      if (-not $env:LOCALAPPDATA) { throw 'install.ps1: LOCALAPPDATA is not set; pass -InstallDir.' }
      $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\tandem'
    }
  }

  # Resolve `latest` through the release page's own redirect, as install.sh
  # does, so the releases API's rate limit is never in the path.
  if ($Version -eq 'latest') {
    if ($localSource) { throw 'install.ps1: a local TANDEM_BASE_URL needs an explicit version.' }
    $tag = ''
    try {
      $req = [Net.HttpWebRequest]::Create("$baseUrl/latest")
      $req.AllowAutoRedirect = $false
      $resp = $req.GetResponse()
      $location = [string]$resp.Headers['Location']
      $resp.Close()
      $tag = ($location -split '/')[-1]
    } catch { $tag = '' }
    if ($tag -notmatch '^v[0-9]') { throw "install.ps1: could not resolve the latest release from $baseUrl/latest" }
  } elseif ($Version.StartsWith('v')) {
    $tag = $Version
  } else {
    $tag = "v$Version"
  }
  $ver = $tag.Substring(1)
  if ($ver -notmatch '^[0-9A-Za-z.-]+$') { throw "install.ps1: '$Version' is not a version this script can install" }
  $archive = "tandem-$ver-windows-x86_64.zip"

  $target = Join-Path $InstallDir 'tandem.exe'
  if ((Test-Path -LiteralPath $target) -and -not $Reinstall) {
    Write-Host "install.ps1: $target already exists; leaving it alone."
    Write-Host "install.ps1: to replace it with $tag, re-run with -Reinstall."
    return
  }

  $tmp = Join-Path ([IO.Path]::GetTempPath()) ('tandem-install-' + [Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    Write-Host "install.ps1: fetching tandem $tag for Windows x86_64 ..."
    foreach ($name in 'SHA256SUMS', $archive) {
      $dest = Join-Path $tmp $name
      if ($localSource) {
        Copy-Item -LiteralPath (Join-Path $baseUrl $name) -Destination $dest
      } else {
        Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/download/$tag/$name" -OutFile $dest
      }
    }

    # The digest line for this archive, and only it: the manifest also covers
    # the other platforms' archives and the installers.
    $digest = $null
    foreach ($line in Get-Content -LiteralPath (Join-Path $tmp 'SHA256SUMS')) {
      $parts = $line.Trim() -split '\s+', 2
      if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -ceq $archive) { $digest = $parts[0]; break }
    }
    if (-not $digest) { throw "install.ps1: SHA256SUMS has no digest line for $archive; refusing." }
    if ($digest -notmatch '^[0-9a-fA-F]{64}$') { throw "install.ps1: the digest for $archive is not 64 hex characters; refusing." }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tmp $archive)).Hash
    if ($actual -ine $digest) { throw "install.ps1: checksum mismatch for $archive; refusing before anything is written." }

    # Extract exactly one named member by stream, never the archive's own
    # paths, so a malformed archive cannot write outside the temp directory.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $extracted = Join-Path $tmp 'tandem.exe'
    $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $tmp $archive))
    try {
      $entry = $zip.Entries | Where-Object { $_.FullName -ceq 'tandem.exe' } | Select-Object -First 1
      if (-not $entry) { throw "install.ps1: $archive carries no tandem.exe; refusing." }
      $in = $entry.Open()
      try {
        $out = [IO.File]::Create($extracted)
        try { $in.CopyTo($out) } finally { $out.Dispose() }
      } finally { $in.Dispose() }
    } finally { $zip.Dispose() }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    try {
      Copy-Item -LiteralPath $extracted -Destination $target -Force
    } catch {
      throw "install.ps1: could not write $target (is tandem serve still running?): $($_.Exception.Message)"
    }
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }

  Add-UserPathEntry $InstallDir

  $printed = & $target --version
  Write-Host "install.ps1: installed $target ($printed)."
  Write-Host 'install.ps1: open a new shell to pick up the PATH change.'
  Write-Host 'install.ps1: next: tandem serve (the board) or tandem doctor (the health check).'
}

function Get-UserPathValue {
  if ($env:TANDEM_FAKE_USER_PATH_FILE) {
    if (Test-Path -LiteralPath $env:TANDEM_FAKE_USER_PATH_FILE) {
      return [string](Get-Content -LiteralPath $env:TANDEM_FAKE_USER_PATH_FILE -Raw)
    }
    return ''
  }
  return [string][Environment]::GetEnvironmentVariable('Path', 'User')
}

function Set-UserPathValue([string]$Value) {
  if ($env:TANDEM_FAKE_USER_PATH_FILE) {
    [IO.File]::WriteAllText($env:TANDEM_FAKE_USER_PATH_FILE, $Value)
  } else {
    [Environment]::SetEnvironmentVariable('Path', $Value, 'User')
  }
}

# Add the folder to the user PATH only when absent (compared without a trailing
# backslash, case-insensitively), and to this process so the version check and
# an immediate `tandem` work.
function Add-UserPathEntry([string]$Dir) {
  $norm = $Dir.TrimEnd('\')
  $current = (Get-UserPathValue).Trim()
  $entries = @($current -split ';' | Where-Object { $_ })
  if (-not ($entries | Where-Object { $_.TrimEnd('\') -ieq $norm })) {
    $new = if ($current) { $current.TrimEnd(';') + ';' + $norm } else { $norm }
    Set-UserPathValue $new
    Write-Host "install.ps1: added $norm to your user PATH."
  }
  if (-not (@($env:Path -split ';') | Where-Object { $_.TrimEnd('\') -ieq $norm })) {
    $env:Path = "$env:Path;$norm"
  }
}

try {
  Install-Tandem -Version $Version -InstallDir $InstallDir -Reinstall ([bool]$Reinstall)
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  # Under `| iex` there is no script path, and `exit` would close the user's
  # shell: report through $LASTEXITCODE instead. Run as a file, exit 1.
  if ($MyInvocation.MyCommand.Path) { exit 1 }
  $global:LASTEXITCODE = 1
}
