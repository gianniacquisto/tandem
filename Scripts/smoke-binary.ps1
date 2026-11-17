# The Windows archive smoke (CUT-16, moved out of release.yml by CUT-32): extract
# the zip the job packaged and run the executable it carries, so a member-name
# or layout change fails the build. The PowerShell twin of smoke-binary.sh.
#
# Usage: Scripts/smoke-binary.ps1 -Archive <tandem.zip> -Version <expected>
param(
  [Parameter(Mandatory = $true)][string]$Archive,
  [Parameter(Mandatory = $true)][string]$Version
)
$ErrorActionPreference = 'Stop'

$temp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$smoke = Join-Path $temp 'smoke'
New-Item -ItemType Directory -Force -Path $smoke | Out-Null
Expand-Archive -Path $Archive -DestinationPath $smoke -Force
$exe = (Resolve-Path (Join-Path $smoke 'tandem.exe')).Path
$printed = & $exe --version
if ($printed -notmatch [regex]::Escape($Version)) {
  throw "tandem --version did not name $Version"
}
$smokeHome = Join-Path $temp 'smoke-home'
New-Item -ItemType Directory -Force -Path $smokeHome | Out-Null
$out = Join-Path $temp 'serve.log'
$err = Join-Path $temp 'serve.err'
$process = Start-Process -FilePath $exe `
  -ArgumentList @('serve', '--port', '0', '--home', $smokeHome) `
  -RedirectStandardOutput $out -RedirectStandardError $err -PassThru -NoNewWindow
$address = $null
for ($i = 0; $i -lt 150; $i++) {
  Start-Sleep -Milliseconds 100
  if (Test-Path $out) {
    $match = Select-String -Path $out -Pattern 'http://[0-9.]+:[0-9]+' -ErrorAction SilentlyContinue |
      Select-Object -First 1
    if ($match) { $address = $match.Matches[0].Value; break }
  }
  if ($process.HasExited) { throw "the server exited before it listened" }
}
if (-not $address) { throw "the server did not print its address" }
foreach ($path in @('/', '/notes')) {
  $response = Invoke-WebRequest -Uri "$address$path" -UseBasicParsing
  if ($response.StatusCode -ne 200) { throw "$path answered $($response.StatusCode)" }
  if (-not $response.Headers['Content-Security-Policy']) { throw "$path carries no Content-Security-Policy" }
}
Stop-Process -Id $process.Id -Force
