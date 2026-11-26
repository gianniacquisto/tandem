# Tests for Scripts/install.ps1, run under Windows PowerShell 5.1 (powershell.exe)
# in release.yml's Windows build job against the zip that job just packaged. The
# installer runs as a user runs it, in a child powershell.exe, against a local
# directory standing in for the release (TANDEM_BASE_URL) with a temp
# LOCALAPPDATA, and a file standing in for the user PATH value
# (TANDEM_FAKE_USER_PATH_FILE), so the runner's real PATH is never edited.
#
# Usage: Scripts/test_install.ps1 -Archive <tandem-X.Y.Z-windows-x86_64.zip> -Version <X.Y.Z>
param(
  [Parameter(Mandatory = $true)][string]$Archive,
  [Parameter(Mandatory = $true)][string]$Version
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

$installer = Join-Path $PSScriptRoot 'install.ps1'
$archivePath = (Resolve-Path $Archive).Path
$archiveName = Split-Path -Leaf $archivePath
$root = Join-Path ([IO.Path]::GetTempPath()) ('tandem-test-install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null

$script:pass = 0
$script:fail = 0
function Ok([string]$Name) { $script:pass++; Write-Host "ok: $Name" }
function No([string]$Name) { $script:fail++; Write-Host "NOT OK: $Name" }
function Check([bool]$Cond, [string]$Name) { if ($Cond) { Ok $Name } else { No $Name } }

function Sha([string]$Path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLower() }

# A release directory: the zip, and a SHA256SUMS in `sha256sum`'s format that
# also lists a file this machine never downloads, as the real manifest does.
function New-Release([string]$Dir, [string]$Zip, [string]$SumsFor) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  Copy-Item -LiteralPath $Zip -Destination (Join-Path $Dir $archiveName)
  $lines = @(
    ('{0}  tandem-{1}-linux-x86_64.tar.gz' -f ('0' * 64), $Version),
    ('{0}  {1}' -f (Sha $SumsFor), $archiveName),
    ('{0}  install.sh' -f ('1' * 64))
  )
  [IO.File]::WriteAllText((Join-Path $Dir 'SHA256SUMS'), (($lines -join "`n") + "`n"))
}

# Run install.ps1 in a child Windows PowerShell with its own LOCALAPPDATA.
function Invoke-Installer([string]$Release, [string]$Local, [string]$PathFile, [string[]]$Extra) {
  $saved = @{}
  foreach ($k in 'LOCALAPPDATA', 'TANDEM_BASE_URL', 'TANDEM_FAKE_USER_PATH_FILE', 'TANDEM_INSTALL_DIR', 'TANDEM_VERSION') {
    $saved[$k] = [Environment]::GetEnvironmentVariable($k, 'Process')
  }
  $env:LOCALAPPDATA = $Local
  $env:TANDEM_BASE_URL = $Release
  $env:TANDEM_FAKE_USER_PATH_FILE = $PathFile
  Remove-Item Env:\TANDEM_INSTALL_DIR -ErrorAction SilentlyContinue
  Remove-Item Env:\TANDEM_VERSION -ErrorAction SilentlyContinue
  # Under 'Stop', Windows PowerShell 5.1 turns the child's stderr lines into
  # terminating errors; the installer reports failures on stderr by design.
  $ErrorActionPreference = 'Continue'
  try {
    $text = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer -Version $Version @Extra 2>&1 | Out-String
    $code = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = 'Stop'
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
  }
  return [pscustomobject]@{ Code = $code; Text = $text }
}

function Path-Count([string]$PathFile, [string]$Dir) {
  if (-not (Test-Path -LiteralPath $PathFile)) { return 0 }
  $entries = (Get-Content -LiteralPath $PathFile -Raw) -split ';' | Where-Object { $_.Trim() -ieq $Dir }
  return @($entries).Count
}

try {
  # The reference bytes of tandem.exe inside the zip.
  $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
  $refExe = Join-Path $root 'ref-tandem.exe'
  try {
    $entry = $zip.Entries | Where-Object { $_.FullName -ceq 'tandem.exe' } | Select-Object -First 1
    [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $refExe)
  } finally { $zip.Dispose() }
  $refHash = Sha $refExe

  $release = Join-Path $root 'release'
  New-Release $release $archivePath $archivePath

  # --- 1: a fresh install ---
  $local1 = Join-Path $root 'local1'
  $pathFile1 = Join-Path $root 'userpath1.txt'
  [IO.File]::WriteAllText($pathFile1, 'C:\Existing\bin')
  $dir1 = Join-Path $local1 'Programs\tandem'
  $r1 = Invoke-Installer $release $local1 $pathFile1 @()
  Check ($r1.Code -eq 0) 'a fresh install exits zero'
  Check (Test-Path -LiteralPath (Join-Path $dir1 'tandem.exe')) 'tandem.exe lands in %LOCALAPPDATA%\Programs\tandem'
  $printed = & (Join-Path $dir1 'tandem.exe') --version
  Check ("$printed" -match [regex]::Escape($Version)) "the installed tandem.exe --version names $Version"
  Check ((Path-Count $pathFile1 $dir1) -eq 1) 'the user PATH gains the folder once'
  Check ((Get-Content -LiteralPath $pathFile1 -Raw) -like 'C:\Existing\bin;*') 'the existing user PATH entries are kept'
  Check ($r1.Text -match 'new shell') 'the output notes that a new shell picks up the PATH'

  # --- 2: a rerun leaves the install alone and never duplicates PATH ---
  Add-Content -LiteralPath (Join-Path $dir1 'tandem.exe') -Value 'local-edit' -Encoding Ascii
  $edited = Sha (Join-Path $dir1 'tandem.exe')
  $r2 = Invoke-Installer $release $local1 $pathFile1 @()
  Check ($r2.Code -eq 0) 'a rerun exits zero'
  Check ($r2.Text -match 'already exists') 'a rerun says the install already exists'
  Check ((Sha (Join-Path $dir1 'tandem.exe')) -eq $edited) 'a rerun without -Reinstall leaves the binary alone'
  Check ((Path-Count $pathFile1 $dir1) -eq 1) 'a rerun does not duplicate the PATH entry'

  # --- 4: -Reinstall replaces ---
  $r3 = Invoke-Installer $release $local1 $pathFile1 @('-Reinstall')
  Check ($r3.Code -eq 0) '-Reinstall exits zero'
  Check ((Sha (Join-Path $dir1 'tandem.exe')) -eq $refHash) '-Reinstall replaces the binary'
  Check ((Path-Count $pathFile1 $dir1) -eq 1) '-Reinstall does not duplicate the PATH entry'

  # --- 3: a tampered zip fails the digest check and installs nothing ---
  $bytes = [IO.File]::ReadAllBytes($archivePath)
  $bytes[[int]($bytes.Length / 2)] = $bytes[[int]($bytes.Length / 2)] -bxor 0xFF
  $tampered = Join-Path $root 'tampered.zip'
  [IO.File]::WriteAllBytes($tampered, $bytes)
  $badRelease = Join-Path $root 'bad-release'
  New-Release $badRelease $archivePath $archivePath
  Copy-Item -LiteralPath $tampered -Destination (Join-Path $badRelease $archiveName) -Force
  $local3 = Join-Path $root 'local3'
  $pathFile3 = Join-Path $root 'userpath3.txt'
  $dir3 = Join-Path $local3 'Programs\tandem'
  $r4 = Invoke-Installer $badRelease $local3 $pathFile3 @()
  Check ($r4.Code -eq 1) 'a tampered zip exits 1'
  Check ($r4.Text -match 'checksum mismatch') 'a tampered zip says checksum mismatch'
  Check (-not (Test-Path -LiteralPath (Join-Path $dir3 'tandem.exe'))) 'a tampered zip installs nothing'
  Check (-not (Test-Path -LiteralPath $dir3)) 'a tampered zip creates no install directory'
  Check ((Path-Count $pathFile3 $dir3) -eq 0) 'a tampered zip leaves the user PATH alone'

  # --- a manifest with no line for the zip refuses ---
  $noLine = Join-Path $root 'no-line'
  New-Release $noLine $archivePath $archivePath
  [IO.File]::WriteAllText((Join-Path $noLine 'SHA256SUMS'), ('{0}  install.sh' -f ('1' * 64)) + "`n")
  $local5 = Join-Path $root 'local5'
  $r5 = Invoke-Installer $noLine $local5 (Join-Path $root 'userpath5.txt') @()
  Check ($r5.Code -eq 1) 'a manifest without the zip exits 1'
  Check (-not (Test-Path -LiteralPath (Join-Path $local5 'Programs\tandem\tandem.exe'))) 'a manifest without the zip installs nothing'
} finally {
  Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "$script:pass passed, $script:fail failed"
if ($script:fail -gt 0) { exit 1 }
