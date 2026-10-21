<#
.SYNOPSIS
    Measures CodeRadar Studio desktop cold start (bootmark) and idle working set.

.DESCRIPTION
    Measures a desktop executable that has ALREADY been built. This script does
    not build anything and does not invoke cargo; build the binary first, then
    point -ExePath at it.

    Boot phase: each run sets CODERADAR_BOOTMARK=1, captures stderr and waits
    (up to 30 seconds per run) until the probe writes "bootmark=<ms>"; the app
    is then terminated. bootmark is the moment the Tauri event loop is ready,
    not first paint.

    Idle phase: one run sets CODERADAR_BOOTMARK=1 and CODERADAR_IDLE_MS=<IdleMs>.
    PeakWorkingSet64 is sampled every 100 ms until the probe exits on its own
    (or IdleMs + 10 seconds elapse, whichever comes first).

    The probe is compiled into src-tauri/src/main.rs and is inert unless those
    environment variables are set, so normal app behaviour is unchanged.

.PARAMETER ExePath
    Path to the built executable, for example
    src-tauri\target\release\coderadar-desktop.exe

.PARAMETER Runs
    Number of boot samples to collect (default 5).

.PARAMETER IdleMs
    Milliseconds the app stays alive for the idle sample (default 4000).

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\measure-desktop.ps1 -ExePath src-tauri\target\release\coderadar-desktop.exe -Runs 5 -IdleMs 4000

.NOTES
    Exit codes: 0 = success, 1 = no bootmark observed, 2 = executable missing.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$ExePath,

    [ValidateRange(1, 1000)]
    [int]$Runs = 5,

    [ValidateRange(1, 3600000)]
    [int]$IdleMs = 4000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resolvedExe = Resolve-Path -LiteralPath $ExePath -ErrorAction SilentlyContinue
if ($null -eq $resolvedExe) {
    [Console]::Error.WriteLine("error: executable not found: $ExePath")
    exit 2
}
$exeFull = [string]$resolvedExe
if (-not (Test-Path -LiteralPath $exeFull -PathType Leaf)) {
    [Console]::Error.WriteLine("error: executable not found: $ExePath")
    exit 2
}

# A PowerShell scriptblock cannot be used directly as a DataReceivedEventHandler
# under Windows PowerShell 5.1 (the callback runs without a runspace), so the
# asynchronous stderr reader lives in this tiny in-memory C# type.
if ($null -eq ('StderrFileCapture' -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.IO;

public sealed class StderrFileCapture
{
    private readonly string path;
    private readonly object gate = new object();

    public StderrFileCapture(string path)
    {
        this.path = path;
    }

    public void Attach(Process process)
    {
        process.ErrorDataReceived += delegate(object sender, DataReceivedEventArgs e)
        {
            if (e.Data == null)
            {
                return;
            }

            lock (gate)
            {
                try
                {
                    File.AppendAllText(path, e.Data + Environment.NewLine);
                }
                catch (IOException)
                {
                }
            }
        };
        process.BeginErrorReadLine();
    }
}
"@
}

function Stop-ProbeProcess {
    param(
        [Parameter(Mandatory = $true)]
        [System.Diagnostics.Process]$Process
    )

    try {
        if (-not $Process.HasExited) {
            $Process.Kill()
            $null = $Process.WaitForExit(5000)
        }
    }
    catch {
        # The process may exit between the HasExited check and Kill().
    }
    finally {
        $Process.Dispose()
    }
}

function Start-ProbeProcess {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(Mandatory = $true)]
        [string]$StderrPath,

        [Parameter(Mandatory = $true)]
        [hashtable]$Environment
    )

    Remove-Item -LiteralPath $StderrPath -Force -ErrorAction SilentlyContinue

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardError = $true
    foreach ($name in $Environment.Keys) {
        $startInfo.EnvironmentVariables[$name] = [string]$Environment[$name]
    }

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    $capture = New-Object -TypeName StderrFileCapture -ArgumentList $StderrPath

    if (-not $process.Start()) {
        throw "failed to start $FilePath"
    }
    $capture.Attach($process)
    return $process
}

function Get-Bootmark {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StderrPath
    )

    if (-not (Test-Path -LiteralPath $StderrPath)) {
        return $null
    }
    $text = Get-Content -LiteralPath $StderrPath -Raw -ErrorAction SilentlyContinue
    if ($null -eq $text) {
        return $null
    }
    $match = [regex]::Match([string]$text, 'bootmark=(\d+)')
    if (-not $match.Success) {
        return $null
    }
    return [long]($match.Groups[1].Value)
}

$bootRows = New-Object 'System.Collections.Generic.List[object]'
$tempFiles = New-Object 'System.Collections.Generic.List[string]'
$exitCode = 0

try {
    Write-Output ("Boot phase: {0} run(s), 30 s timeout per run" -f $Runs)
    for ($run = 1; $run -le $Runs; $run++) {
        $stderrPath = Join-Path -Path $env:TEMP -ChildPath ("coderadar-bootmark-{0}-{1}.log" -f $PID, $run)
        $tempFiles.Add($stderrPath)
        $process = $null
        $sample = $null

        try {
            $process = Start-ProbeProcess -FilePath $exeFull -StderrPath $stderrPath -Environment @{
                CODERADAR_BOOTMARK = '1'
            }
            $deadline = (Get-Date).AddSeconds(30)
            while ((Get-Date) -lt $deadline) {
                $sample = Get-Bootmark -StderrPath $stderrPath
                if ($null -ne $sample) {
                    break
                }
                if ($process.HasExited) {
                    break
                }
                Start-Sleep -Milliseconds 50
            }
        }
        finally {
            if ($null -ne $process) {
                Stop-ProbeProcess -Process $process
            }
        }

        if ($null -eq $sample) {
            Start-Sleep -Milliseconds 100
            $sample = Get-Bootmark -StderrPath $stderrPath
        }

        if ($null -ne $sample) {
            $bootRows.Add([pscustomobject]@{ Run = $run; Bootmark = $sample })
            Write-Output ("  run {0}: {1} ms" -f $run, $sample)
        }
        else {
            Write-Warning ("run {0}: no bootmark observed" -f $run)
        }
    }

    Write-Output ("Idle phase: CODERADAR_IDLE_MS={0}" -f $IdleMs)
    $idleStderrPath = Join-Path -Path $env:TEMP -ChildPath ("coderadar-idle-{0}.log" -f $PID)
    $tempFiles.Add($idleStderrPath)
    $idlePeakBytes = [long]0
    $idleProcess = $null

    try {
        $idleProcess = Start-ProbeProcess -FilePath $exeFull -StderrPath $idleStderrPath -Environment @{
            CODERADAR_BOOTMARK = '1'
            CODERADAR_IDLE_MS = [string]$IdleMs
        }
        $idleDeadline = (Get-Date).AddMilliseconds($IdleMs + 10000)
        while ((Get-Date) -lt $idleDeadline) {
            $live = Get-Process -Id $idleProcess.Id -ErrorAction SilentlyContinue
            if ($null -eq $live) {
                break
            }
            if ($live.PeakWorkingSet64 -gt $idlePeakBytes) {
                $idlePeakBytes = [long]$live.PeakWorkingSet64
            }
            Start-Sleep -Milliseconds 100
        }
    }
    finally {
        if ($null -ne $idleProcess) {
            Stop-ProbeProcess -Process $idleProcess
        }
    }

    Write-Output ''
    Write-Output ("{0,3}  {1,14}" -f 'Run', 'Bootmark (ms)')
    Write-Output ("{0,3}  {1,14}" -f '---', '-------------')
    foreach ($row in $bootRows) {
        Write-Output ("{0,3}  {1,14}" -f $row.Run, $row.Bootmark)
    }

    $idleMiB = [math]::Round($idlePeakBytes / 1MB, 1)
    Write-Output ("Idle peak working set: {0} MiB" -f $idleMiB)

    if ($bootRows.Count -eq 0) {
        [Console]::Error.WriteLine("error: no bootmark was observed; was $exeFull built with the boot probe?")
        $exitCode = 1
    }
    else {
        $samples = @($bootRows | ForEach-Object { $_.Bootmark })
        $sorted = @($samples | Sort-Object)
        $min = $sorted[0]
        $max = $sorted[$sorted.Count - 1]
        $middle = [int][math]::Floor($sorted.Count / 2)
        if (($sorted.Count % 2) -eq 1) {
            $median = $sorted[$middle]
        }
        else {
            $median = [math]::Round((($sorted[$middle - 1] + $sorted[$middle]) / 2.0), 1)
        }
        Write-Output ("Bootmark min/median/max: {0} / {1} / {2} ms" -f $min, $median, $max)
    }
}
finally {
    foreach ($file in $tempFiles) {
        Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue
    }
}

exit $exitCode
