param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [Parameter(Mandatory)][string[]]$BenchmarkArguments,
    [Parameter(Mandatory)][long]$AffinityMask,
    [switch]$TextOutput
)
$ErrorActionPreference = 'Stop'
$process = [System.Diagnostics.Process]::new()
$process.StartInfo.FileName = (Resolve-Path -LiteralPath $Executable).Path
$process.StartInfo.UseShellExecute = $false
$process.StartInfo.CreateNoWindow = $true
$process.StartInfo.RedirectStandardOutput = $true
$process.StartInfo.RedirectStandardError = $true
foreach ($argument in $BenchmarkArguments) { $process.StartInfo.ArgumentList.Add($argument) }
$timer = [System.Diagnostics.Stopwatch]::StartNew()
if (-not $process.Start()) { throw 'benchmark did not start' }
$process.ProcessorAffinity = [IntPtr]$AffinityMask
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
$peak = 0L
while (-not $process.HasExited) {
    $process.Refresh()
    $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
    Start-Sleep -Milliseconds 25
}
$process.WaitForExit()
$timer.Stop()
$text = $stdout.GetAwaiter().GetResult()
$errors = $stderr.GetAwaiter().GetResult()
$suffix = if ($TextOutput) { 'log' } else { 'jsonl' }
[System.IO.File]::WriteAllText("$Output.$suffix", $text)
[System.IO.File]::WriteAllText("$Output.stderr", $errors)
$rows = @()
if (-not $TextOutput) {
    $rows = @($text -split '\r?\n' | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
}
$receipt = [ordered]@{
    executable = $process.StartInfo.FileName
    executable_sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
    arguments = $BenchmarkArguments
    affinity_mask = $AffinityMask
    wall_ms = $timer.Elapsed.TotalMilliseconds
    peak_working_set_bytes = $peak
    exit_code = $process.ExitCode
    rows = $rows
}
$receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath "$Output.json"
$receipt | ConvertTo-Json -Depth 8
if ($process.ExitCode -ne 0) { throw "benchmark failed: $errors" }
