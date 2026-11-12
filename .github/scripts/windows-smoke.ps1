# Exercise the shipped executable on Windows without touching the source font.
# Only use public fixtures: opt-in CDB minidumps contain unsanitized process memory.
param(
    [switch]$CollectCrashDump,
    [ValidateSet(0, 5, 15)]
    [int]$NativeObservationSeconds = 0
)

$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path 'target/debug/runebender.exe').Path
$font = (Resolve-Path 'test-fonts/sources/VirtuaGrotesk-Regular.ufo').Path
$out = (New-Item -ItemType Directory -Force 'windows-smoke').FullName
$scratch = (New-Item -ItemType Directory (Join-Path ([IO.Path]::GetTempPath()) "runebender-smoke-$([guid]::NewGuid())")).FullName
$diagnostics = [ordered]@{
    dumpRequested = [bool]$CollectCrashDump
    observationSeconds = $NativeObservationSeconds
    commit = $env:GITHUB_SHA
    runId = $env:GITHUB_RUN_ID
    osVersion = [Environment]::OSVersion.VersionString
    notes = [System.Collections.Generic.List[string]]::new()
}

function Protect-DiagnosticText([string]$Text) {
    # Preserve symbols, module versions and offsets, but redact user/workspace paths.
    foreach ($entry in @(
        @($PWD.Path, '<WORKSPACE>'),
        @($scratch, '<DIAGNOSTIC-TEMP>'),
        @([IO.Path]::GetTempPath().TrimEnd('\'), '<TEMP>'),
        @($env:LOCALAPPDATA, '<LOCALAPPDATA>'),
        @($env:USERPROFILE, '<USERPROFILE>')
    )) {
        if ($entry[0]) {
            foreach ($path in @($entry[0], $entry[0].Replace('\', '\\'))) {
                $Text = [regex]::Replace($Text, [regex]::Escape($path), $entry[1], 'IgnoreCase')
            }
        }
    }
    # Handle 8.3 user directories as well as paths escaped inside JSON.
    $Text = [regex]::Replace($Text, '(?i)\b[A-Z]:\\\\Users\\\\[^\\\r\n"<>]+', '<USERPROFILE>')
    return [regex]::Replace($Text, '(?i)\b[A-Z]:\\Users\\[^\\\r\n"<>]+', '<USERPROFILE>')
}

function Export-DiagnosticText([string]$Source, [string]$Name) {
    # Publish at most the last 4 MiB, even if the application produces a pathological log.
    $limit = 4MB
    $stream = [IO.File]::Open($Source, 'Open', 'Read', 'ReadWrite')
    try {
        $length = $stream.Length
        $offset = [Math]::Max(0, $length - $limit)
        [void]$stream.Seek($offset, 'Begin')
        $bytes = [byte[]]::new([int][Math]::Min($length, $limit))
        $read = 0
        while ($read -lt $bytes.Length) {
            $count = $stream.Read($bytes, $read, $bytes.Length - $read)
            if ($count -eq 0) { break }
            $read += $count
        }
        $text = [Text.Encoding]::UTF8.GetString($bytes, 0, $read)
        if ($offset -gt 0) { $text = "[Truncated: last $limit of $length bytes]`n$text" }
        Protect-DiagnosticText $text | Set-Content (Join-Path $out $Name)
    } finally { $stream.Dispose() }
}

function Find-Cdb {
    $command = Get-Command cdb.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { return $command.Source }
    foreach ($root in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        if (!$root) { continue }
        $candidate = Join-Path $root 'Windows Kits/10/Debuggers/x64/cdb.exe'
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    return $null
}

function Export-LiveReproduction([string]$Cdb) {
    # A second execution collects evidence; it never replaces the failed native startup gate.
    # sxd passes first-chance AVs to the app; -c2 captures only an unhandled second-chance AV.
    # https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/sx--sxd--sxe--sxi--sxn--sxr--sx---set-exceptions-
    $images = Split-Path -Parent $exe
    $diagnostics.debuggerPath = $Cdb
    $diagnostics.debuggerVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($Cdb).FileVersion
    $diagnostics.localPdbAvailable = Test-Path ([IO.Path]::ChangeExtension($exe, '.pdb'))
    $diagnostics.reproductionOutcome = 'incomplete'
    $diagnostics.reproductionCapturedSecondChanceAv = $false
    [void](New-Item -ItemType Directory -Force 'windows-smoke-dumps')
    # Relative dump naming avoids nested quotes while preserving the original working directory.
    # At a live second-chance stop, registers already belong to the faulting thread.
    # Write the minidump before symbol downloads or thread switching.
    $capture = @(
        '.echo RUNEBENDER_SECOND_CHANCE_AV',
        '.exr -1', 'r',
        '.dump /mR windows-smoke-dumps/pending.dmp',
        '.reload /f runebender.exe',
        'kv 0n64', '~* kb 0n32', 'lmv',
        '!analyze -v', 'q'
    ) -join '; '
    @('version', "sxd -c2 `"$capture`" av", 'sx', 'g') |
        Set-Content "$scratch/cdb.commands" -Encoding ascii
    $symbols = "$images;srv*$scratch/symbols*https://msdl.microsoft.com/download/symbols"
    $debugger = $null
    $target = $null
    $closeAt = $null
    $closeRequested = $false
    try {
        $debugger = Start-Process -FilePath $Cdb -ArgumentList @(
            '-i', "`"$images`"", '-y', "`"$symbols`"",
            '-lines', '-sins', '-ses', '-noshell', '-nosqm', '-G',
            '-logo', "`"$scratch/cdb.log`"", '-cf', "`"$scratch/cdb.commands`"",
            "`"$exe`"", "`"$font`""
        ) -PassThru -NoNewWindow -RedirectStandardOutput "$scratch/cdb.stdout.txt" `
            -RedirectStandardError "$scratch/cdb.stderr.txt"
        [void]$debugger.Handle
        $deadline = (Get-Date).AddSeconds(120)
        while (!$debugger.HasExited -and (Get-Date) -lt $deadline) {
            if (!$target) {
                # Parent identity prevents touching any unrelated process/window on the runner.
                $child = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($debugger.Id) AND Name = 'runebender.exe'" `
                    -OperationTimeoutSec 3 | Select-Object -First 1
                if ($child) {
                    $target = Get-Process -Id $child.ProcessId -ErrorAction SilentlyContinue
                    if ($target) {
                        [void]$target.Handle
                        $diagnostics.reproductionProcessId = $target.Id
                    }
                }
            }
            if ($target) {
                $target.Refresh()
                if (!$target.HasExited -and !$closeRequested -and $target.MainWindowHandle -ne 0 -and $target.MainWindowTitle -eq 'Runebender') {
                    if ($null -eq $closeAt) { $closeAt = (Get-Date).AddSeconds($NativeObservationSeconds) }
                    if ((Get-Date) -ge $closeAt) {
                        $closeRequested = $true
                        $diagnostics.reproductionCloseRequested = $target.CloseMainWindow()
                    }
                }
            }
            Start-Sleep -Milliseconds 500
            $debugger.Refresh()
        }
        if (!$debugger.HasExited) {
            $diagnostics.reproductionOutcome = 'timed-out'
            throw 'CDB reproduction exceeded two minutes; timeout does not establish successful startup or a reproduced crash'
        }
        $diagnostics.reproductionDebuggerExitCode = $debugger.ExitCode
        $diagnostics.reproductionOutcome = 'exited-without-confirmed-second-chance-access-violation'
    } finally {
        if ($debugger) {
            if (!$debugger.HasExited) {
                $debugger.Kill($true)
                [void]$debugger.WaitForExit(5000)
            }
            $debugger.Dispose()
        }
        if ($target) {
            if (!$target.HasExited) {
                $target.Kill()
                [void]$target.WaitForExit(5000)
            }
            $target.Dispose()
        }
        $logPath = if (Test-Path "$scratch/cdb.log") { "$scratch/cdb.log" } else { "$scratch/cdb.stdout.txt" }
        if (Test-Path $logPath) { Export-DiagnosticText $logPath 'native-debugger.log' }
        if (Test-Path "$scratch/cdb.stderr.txt") { Export-DiagnosticText "$scratch/cdb.stderr.txt" 'native-debugger.stderr.txt' }
        $log = [string](Get-Content "$out/native-debugger.log" -Raw -ErrorAction SilentlyContinue)
        # Require an executed marker line, not its occurrence in the echoed filter command.
        $captured = $log -match '(?m)^RUNEBENDER_SECOND_CHANCE_AV\s*$'
        $diagnostics.reproductionCapturedSecondChanceAv = $captured
        if ($captured -and $diagnostics.reproductionOutcome -ne 'timed-out') {
            $diagnostics.reproductionOutcome = 'second-chance-access-violation'
        }
        $dump = Get-Item -LiteralPath 'windows-smoke-dumps/pending.dmp' -ErrorAction SilentlyContinue
        if ($dump) {
            if ($captured -and $log.Contains('Dump successfully written') -and $dump.Length -gt 0 -and $dump.Length -le 64MB) {
                # Only this validated, size-bounded filename is eligible for workflow upload.
                Copy-Item -LiteralPath $dump.FullName -Destination 'windows-smoke-dumps/runebender.dmp'
                $diagnostics.dumpBytes = $dump.Length
                $diagnostics.dumpSha256 = (Get-FileHash $dump.FullName -Algorithm SHA256).Hash
            } else {
                $diagnostics.notes.Add('Minidump incomplete, unconfirmed, or larger than 64 MiB; not uploaded.')
            }
            Remove-Item -LiteralPath $dump.FullName -Force
        }
        if (!$captured) { $diagnostics.notes.Add('The bounded debugger reproduction did not capture a second-chance access violation.') }
    }
}

$app = $null
try {
    & $exe --version
    if ($LASTEXITCODE -ne 0) { throw 'Version command failed' }
    $info = & $exe info $font --json
    if ($LASTEXITCODE -ne 0) { throw 'Font inspection failed' }
    Protect-DiagnosticText ($info -join "`n") | Set-Content "$out/info.json"
    $data = $info | ConvertFrom-Json
    if (!$data.ok -or $data.glyphs -le 0) { throw 'Font inspection returned no glyphs' }
    & $exe proof $font --glyphs H,n,o --out "$out/proof.svg" --json
    if ($LASTEXITCODE -ne 0) { throw 'SVG proof failed' }
    $env:RUNEBENDER_SCREENSHOT = "$out/editor.png"
    $env:RUNEBENDER_SIZE = '1100x720'
    try {
        & $exe $font
        if ($LASTEXITCODE -ne 0) { throw 'Editor rendering failed' }
        if (!(Test-Path "$out/editor.png")) { throw 'Editor screenshot is missing' }
    } finally {
        Remove-Item Env:RUNEBENDER_SCREENSHOT -ErrorAction SilentlyContinue
        Remove-Item Env:RUNEBENDER_SIZE -ErrorAction SilentlyContinue
    }

    # A real window remains a separate gate from the CPU rendering path above.
    $diagnostics.executableSha256 = (Get-FileHash $exe -Algorithm SHA256).Hash
    $app = Start-Process -FilePath $exe -ArgumentList "`"$font`"" -PassThru -NoNewWindow `
        -RedirectStandardOutput "$scratch/window.stdout.txt" -RedirectStandardError "$scratch/window.stderr.txt"
    # Retain the process handle so ExitCode survives a fast crash.
    [void]$app.Handle
    $diagnostics.processId = $app.Id
    try {
        $deadline = (Get-Date).AddSeconds(45)
        do {
            Start-Sleep -Milliseconds 500
            $app.Refresh()
            if ($app.HasExited) { throw "Native application exited early: $($app.ExitCode)" }
        } while (($app.MainWindowHandle -eq 0 -or $app.MainWindowTitle -ne 'Runebender') -and (Get-Date) -lt $deadline)
        if ($app.MainWindowHandle -eq 0 -or $app.MainWindowTitle -ne 'Runebender') { throw 'No native Runebender window appeared' }
        "Native window: $($app.MainWindowHandle), title: $($app.MainWindowTitle)" | Set-Content "$out/native-window.txt"
        $observationDeadline = (Get-Date).AddSeconds($NativeObservationSeconds)
        while ((Get-Date) -lt $observationDeadline) {
            Start-Sleep -Milliseconds 250
            $app.Refresh()
            if ($app.HasExited) { throw "Native application exited during observation: $($app.ExitCode)" }
        }
        if (!$app.CloseMainWindow()) { throw 'Window refused the close request' }
        if (!$app.WaitForExit(15000)) { throw 'Window did not close cleanly' }
        if ($app.ExitCode -ne 0) { throw "Native application failed: $($app.ExitCode)" }
    } finally {
        $app.Refresh()
        $forced = !$app.HasExited
        if ($forced) {
            $app.Kill()
            if (!$app.WaitForExit(5000)) { $diagnostics.notes.Add('Native process did not exit within five seconds of forced termination.') }
        }
        $diagnostics.forcedTermination = $forced
        if ($app.HasExited) {
            $diagnostics.exitCode = $app.ExitCode
            $diagnostics.exitCodeHex = '0x{0:X8}' -f ($app.ExitCode -band 0xFFFFFFFFL)
        }
        foreach ($name in @('window.stdout.txt', 'window.stderr.txt')) {
            if (Test-Path "$scratch/$name") { Export-DiagnosticText "$scratch/$name" $name }
        }
        # Select only the exact named Masonry log inside the process temp directory.
        $stderr = Get-Content "$out/window.stderr.txt" -Raw -ErrorAction SilentlyContinue
        $pointer = [regex]::Match([string]$stderr, '(?m)^Writing full logs to ([^\r\n]+)')
        if ($pointer.Success) {
            $name = [IO.Path]::GetFileName($pointer.Groups[1].Value.Trim())
            $dense = Join-Path ([IO.Path]::GetTempPath()) $name
            if ($name -match '^masonry-\d+-dense\.log$' -and (Test-Path -LiteralPath $dense -PathType Leaf)) {
                Export-DiagnosticText $dense 'masonry-dense.log'
            } else { $diagnostics.notes.Add('The referenced Masonry dense log was not found in the process temp directory.') }
        } else { $diagnostics.notes.Add('Native stderr did not contain a Masonry dense log pointer.') }

        if ($CollectCrashDump -and ($forced -or ($app.HasExited -and $app.ExitCode -ne 0))) {
            $cdb = Find-Cdb
            $diagnostics.debuggerAvailable = [bool]$cdb
            if ($cdb) {
                try { Export-LiveReproduction $cdb } catch {
                    $diagnostics.notes.Add("Live debugger reproduction failed: $_")
                    Write-Warning 'Diagnostic reproduction did not complete; original native failure is preserved.'
                }
            } else {
                $diagnostics.reproductionOutcome = 'debugger-unavailable'
                $diagnostics.notes.Add('No installed x64 CDB was found on PATH or in Windows Kits; diagnostic reproduction was not run.')
                Write-Warning 'CDB is unavailable; original native failure is preserved.'
            }
        }
    }
} finally {
    if ($app) { $app.Dispose() }
    Protect-DiagnosticText ($diagnostics | ConvertTo-Json -Depth 6) | Set-Content "$out/diagnostic-status.json"
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
