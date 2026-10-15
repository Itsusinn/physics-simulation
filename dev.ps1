param(
    [ValidateSet('serve', 'build')]
    [string]$Action = 'serve',
    [switch]$Release
)
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$localTrunk = Join-Path $projectRoot '.tools\trunk.exe'
$trunkCommand = if (Test-Path -LiteralPath $localTrunk) { $localTrunk } else { (Get-Command trunk -ErrorAction Stop).Source }
$oldNoColor = $env:NO_COLOR
$oldJobs = $env:CARGO_BUILD_JOBS
Push-Location -LiteralPath $projectRoot
try {
    # Trunk's boolean parser does not accept the conventional NO_COLOR=1.
    $env:NO_COLOR = 'true'
    $env:CARGO_BUILD_JOBS = '2'
    $trunkArgs = @($Action)
    if ($Release) { $trunkArgs += '--release' }
    & $trunkCommand @trunkArgs
    if ($LASTEXITCODE -ne 0) { throw "Trunk failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
    $env:NO_COLOR = $oldNoColor
    $env:CARGO_BUILD_JOBS = $oldJobs
}
