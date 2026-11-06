param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Native,
    [Parameter(Mandatory)][string]$Shared,
    [Parameter(Mandatory)][string]$Bitpacked,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Trace
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a fresh output directory' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$cases = [ordered]@{ native = $Native; shared = $Shared; bitpacked = $Bitpacked }
$replacements = @{}
$identities = @{}
foreach ($name in $cases.Keys) {
    $path = $cases[$name]
    $replacement = Join-Path $OutputDirectory "replacement-$name"
    Copy-Item -LiteralPath $path -Destination $replacement
    $hash = (Get-FileHash -LiteralPath $path).Hash
    if ($hash -ne (Get-FileHash -LiteralPath $replacement).Hash) { throw 'Replacement copy mismatch' }
    $replacements[$name] = $replacement
    $identities[$name] = @{ sha256 = $hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $path).Length }
}
if ($Trace) {
    $identities.trace = @{ sha256 = (Get-FileHash -LiteralPath $Trace).Hash.ToLowerInvariant() }
}
$identities | ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'inputs.json')

function Invoke-Case([string]$Label, [string[]]$Arguments) {
    & "$PSScriptRoot/run.ps1" -Executable $Executable `
        -Output (Join-Path $OutputDirectory $Label) -BenchmarkArguments $Arguments | Out-Null
}

function Read-IntoCache([string]$Path) {
    $file = [System.IO.File]::OpenRead((Resolve-Path -LiteralPath $Path).Path)
    try { $file.CopyTo([System.IO.Stream]::Null) } finally { $file.Dispose() }
}

foreach ($name in @('shared', 'bitpacked')) {
    $argsForCase = @('parity', $Native, $cases[$name])
    if ($Trace) { $argsForCase += $Trace }
    Invoke-Case "parity-$name" $argsForCase
}
for ($trial = 0; $trial -lt 3; $trial++) {
    $order = switch ($trial) {
        0 { @('native', 'shared', 'bitpacked') }
        1 { @('bitpacked', 'shared', 'native') }
        2 { @('shared', 'native', 'bitpacked') }
    }
    foreach ($name in $order) {
        $mode = if ($name -eq 'native') { 'native' } else { 'converted' }
        $argsForCase = @($mode, $Native, $cases[$name])
        if ($Trace) { $argsForCase += $Trace }
        Read-IntoCache $cases[$name]
        Invoke-Case "serve-$name-$trial" $argsForCase
        Read-IntoCache $cases[$name]
        Read-IntoCache $replacements[$name]
        Invoke-Case "reload-$name-$trial" @("reload-$mode", $Native, $cases[$name], $replacements[$name])
    }
}
