# Exercise the shipped executable on Windows without touching the source font.
# Only use public fixtures: opt-in minidumps contain unsanitized process memory.
param(
    [switch]$CollectCrashDump,
    [switch]$InstallDebuggingTools,
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
    dumpEnabled = $false
    observationSeconds = $NativeObservationSeconds
    notes = [System.Collections.Generic.List[string]]::new()
}

function Protect-DiagnosticText([string]$Text) {
    # Keep module names, versions, offsets and exception codes; redact machine/user paths.
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
    # Masonry can print an 8.3 user directory, unlike the environment's long path.
    $Text = [regex]::Replace($Text, '(?i)\b[A-Z]:\\\\Users\\\\[^\\\r\n"<>]+', '<USERPROFILE>')
    return [regex]::Replace($Text, '(?i)\b[A-Z]:\\Users\\[^\\\r\n"<>]+', '<USERPROFILE>')
}

function Export-DiagnosticText([string]$Source, [string]$Name) {
    # Read only the tail, including for a pathological dense log. Never upload the source file.
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
    } finally {
        $stream.Dispose()
    }
}

function Export-NativeEvents([datetime]$Since, [int]$ProcessId) {
    # Structured EventData preserves the actual faulting module, version, offset and code.
    # Do not infer a renderer fault from a hand-maintained DLL category list.
    $queryErrors = @()
    $events = @(Get-WinEvent -FilterHashtable @{
        LogName = 'Application'; StartTime = $Since; Id = 1000, 1001
    } -MaxEvents 64 -ErrorAction SilentlyContinue -ErrorVariable queryErrors)
    $records = @()
    foreach ($event in $events) {
        if ($event.ProviderName -notin @('Application Error', 'Windows Error Reporting')) { continue }
        $xml = [xml]$event.ToXml()
        $fields = @($xml.Event.EventData.Data | ForEach-Object {
            [pscustomobject]@{ name = [string]$_.Name; value = [string]$_.'#text' }
        })
        $appNames = @($fields | Where-Object { $_.name -in @('AppName', 'AppPath', 'P1') })
        if (!($appNames | Where-Object { [IO.Path]::GetFileName($_.value) -ieq 'runebender.exe' })) { continue }
        $eventPid = $fields | Where-Object { $_.name -ieq 'ProcessId' } | Select-Object -First 1
        $correlation = 'application name and collection time window (no process ID in event)'
        if ($eventPid) {
            $value = $eventPid.value
            try {
                $parsed = if ($value -match '^0x') { [Convert]::ToInt64($value.Substring(2), 16) } else { [long]$value }
                if ($parsed -ne $ProcessId) { continue }
                $correlation = 'application name, process ID and collection time window'
            } catch {
                $correlation = 'application name and collection time window (unparseable process ID)'
            }
        }
        # EventData is complete; omit Computer/Security identities and localized message text.
        $records += [pscustomobject]@{
            provider = $event.ProviderName
            id = $event.Id
            recordId = $event.RecordId
            timeCreatedUtc = $event.TimeCreated.ToUniversalTime().ToString('o')
            correlation = $correlation
            data = $fields
        }
    }
    $result = [ordered]@{
        processId = $ProcessId
        sinceUtc = $Since.ToUniversalTime().ToString('o')
        collectedUtc = [datetime]::UtcNow.ToString('o')
        queryLimit = 64
        queryLimitReached = ($events.Count -eq 64)
        queryErrors = @($queryErrors | ForEach-Object { $_.ToString() })
        events = $records
    }
    Protect-DiagnosticText ($result | ConvertTo-Json -Depth 8) | Set-Content "$out/native-events.json"
    return $records.Count
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

function Initialize-Cdb {
    $cdb = Find-Cdb
    if ($cdb -or !$InstallDebuggingTools) { return $cdb }
    # Microsoft's SDK download table identifies this as SDK 10.0.26100.9457:
    # https://learn.microsoft.com/en-us/windows/apps/windows-sdk/downloads
    # Install only Debugging Tools, never alter the application's renderer/dependencies.
    $url = 'https://go.microsoft.com/fwlink/?linkid=2382321'
    $installer = "$scratch/winsdksetup.exe"
    $diagnostics.debuggerInstallerUrl = $url
    Invoke-WebRequest -Uri $url -OutFile $installer -TimeoutSec 60 -MaximumRedirection 3
    if ((Get-Item -LiteralPath $installer).Length -gt 8MB) { throw 'SDK bootstrapper exceeded the 8 MiB limit' }
    $signature = Get-AuthenticodeSignature -LiteralPath $installer
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
        throw 'SDK bootstrapper does not have a valid Microsoft Corporation Authenticode signature'
    }
    $diagnostics.debuggerInstallerSha256 = (Get-FileHash $installer -Algorithm SHA256).Hash
    $diagnostics.debuggerInstallerSigner = $signature.SignerCertificate.Thumbprint
    $setup = $null
    try {
        $setup = Start-Process -FilePath $installer -ArgumentList @(
            '/quiet', '/norestart', '/ceip', 'off', '/features', 'OptionId.WindowsDesktopDebuggers',
            '/log', "`"$scratch/sdk-install.log`""
        ) -PassThru
        [void]$setup.Handle
        if (!$setup.WaitForExit(180000)) { throw 'Debugging Tools installation exceeded three minutes' }
        $diagnostics.debuggerInstallerExitCode = $setup.ExitCode
        if ($setup.ExitCode -notin @(0, 3010)) { throw "Debugging Tools installer failed: $($setup.ExitCode)" }
    } finally {
        if ($setup) {
            if (!$setup.HasExited) {
                $setup.Kill($true)
                [void]$setup.WaitForExit(5000)
            }
            $setup.Dispose()
        }
        if (Test-Path "$scratch/sdk-install.log") { Export-DiagnosticText "$scratch/sdk-install.log" 'debugger-install.log' }
    }
    $cdb = Find-Cdb
    if (!$cdb) { throw 'Debugging Tools installation completed but x64 cdb.exe was not found' }
    return $cdb
}

function Export-DumpAnalysis([string]$Cdb, [string]$DumpPath) {
    $images = Split-Path -Parent $exe
    $diagnostics.debuggerPath = $Cdb
    $diagnostics.debuggerVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($Cdb).FileVersion
    $diagnostics.localPdbAvailable = Test-Path ([IO.Path]::ChangeExtension($exe, '.pdb'))
    $diagnostics.symbolServer = 'https://msdl.microsoft.com/download/symbols'
    # Analyze before the runner removes the exact build's EXE/PDB. Never accept mismatched PDBs.
    # Emit exception/context/stack/module evidence before the potentially slower !analyze command.
    @(
        '.version',
        '.reload /f runebender.exe',
        '.exr -1',
        '.ecxr',
        'r',
        'kv 0n64',
        '~* kb 0n32',
        'lmv',
        '!analyze -v',
        'q'
    ) | Set-Content "$scratch/cdb.commands" -Encoding ascii
    $symbols = "$images;srv*$scratch/symbols*https://msdl.microsoft.com/download/symbols"
    $debugger = $null
    try {
        $debugger = Start-Process -FilePath $Cdb -ArgumentList @(
            '-z', "`"$DumpPath`"", '-i', "`"$images`"", '-y', "`"$symbols`"",
            '-lines', '-sins', '-ses', '-noshell', '-nosqm',
            '-logo', "`"$scratch/cdb.log`"", '-cf', "`"$scratch/cdb.commands`""
        ) -PassThru -NoNewWindow -RedirectStandardOutput "$scratch/cdb.stdout.txt" `
            -RedirectStandardError "$scratch/cdb.stderr.txt"
        [void]$debugger.Handle
        if (!$debugger.WaitForExit(120000)) { throw 'CDB analysis exceeded two minutes; partial evidence is retained' }
        $diagnostics.debuggerExitCode = $debugger.ExitCode
        if ($debugger.ExitCode -ne 0) { throw "CDB analysis failed: $($debugger.ExitCode)" }
    } finally {
        if ($debugger) {
            if (!$debugger.HasExited) {
                $debugger.Kill($true)
                [void]$debugger.WaitForExit(5000)
            }
            $debugger.Dispose()
        }
        if (Test-Path "$scratch/cdb.log") {
            Export-DiagnosticText "$scratch/cdb.log" 'native-debugger.log'
        } elseif (Test-Path "$scratch/cdb.stdout.txt") {
            Export-DiagnosticText "$scratch/cdb.stdout.txt" 'native-debugger.log'
        }
        if (Test-Path "$scratch/cdb.stderr.txt") {
            Export-DiagnosticText "$scratch/cdb.stderr.txt" 'native-debugger.stderr.txt'
        }
    }
}

$registry = $null
$oldValues = @{}
$registryCreated = $false
$registryTouched = $false
$registryPath = 'SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\runebender.exe'
$app = $null
$cdb = $null
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

    if ($CollectCrashDump) {
        try {
            $cdb = Initialize-Cdb
            $diagnostics.debuggerAvailable = [bool]$cdb
            if (!$cdb) { $diagnostics.notes.Add('CDB is unavailable; enable install_debugging_tools for in-runner symbolized analysis.') }
        } catch {
            $diagnostics.debuggerAvailable = $false
            $diagnostics.notes.Add("Debugger preparation failed: $_")
            Write-Warning 'Debugger preparation failed; WER collection and the native gate will still run.'
        }
        try {
            # Hosted Windows runners are administrators. Scope WER to this executable only.
            # DumpType=1 is a minidump, not a full heap dump; keep at most one in a private temp dir.
            $registry = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($registryPath, $true)
            if ($null -eq $registry) {
                $registry = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey($registryPath)
                $registryCreated = $true
            }
            foreach ($name in @('DumpFolder', 'DumpType', 'DumpCount')) {
                if ($name -in $registry.GetValueNames()) {
                    $oldValues[$name] = @{
                        value = $registry.GetValue($name, $null, 'DoNotExpandEnvironmentNames')
                        kind = $registry.GetValueKind($name)
                    }
                }
            }
            $registryTouched = $true
            $registry.SetValue('DumpFolder', $scratch, 'ExpandString')
            $registry.SetValue('DumpType', 1, 'DWord')
            $registry.SetValue('DumpCount', 1, 'DWord')
            $diagnostics.dumpEnabled = $true
        } catch {
            $diagnostics.notes.Add("WER LocalDumps setup failed: $_")
            Write-Warning 'WER dump setup failed; native startup will still run. See diagnostic-status.json.'
        }
    }

    # A real window remains a separate gate from the CPU rendering path above.
    # Redirect directly to private files, avoiding unbounded in-memory stdout/stderr tasks.
    $diagnostics.executableSha256 = (Get-FileHash $exe -Algorithm SHA256).Hash
    $diagnostics.commit = $env:GITHUB_SHA
    $diagnostics.runId = $env:GITHUB_RUN_ID
    $diagnostics.osVersion = [Environment]::OSVersion.VersionString
    $nativeStarted = Get-Date
    $app = Start-Process -FilePath $exe -ArgumentList "`"$font`"" -PassThru -NoNewWindow `
        -RedirectStandardOutput "$scratch/window.stdout.txt" -RedirectStandardError "$scratch/window.stderr.txt"
    # Retain the native process handle so ExitCode remains available after a fast crash.
    [void]$app.Handle
    $diagnostics.processId = $app.Id
    $diagnostics.startedUtc = $nativeStarted.ToUniversalTime().ToString('o')
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
            Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
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

        # The exact stderr pointer selects the dense log. Never follow arbitrary paths from logs.
        $stderr = Get-Content "$out/window.stderr.txt" -Raw -ErrorAction SilentlyContinue
        $pointer = [regex]::Match([string]$stderr, '(?m)^Writing full logs to ([^\r\n]+)')
        if ($pointer.Success) {
            $name = [IO.Path]::GetFileName($pointer.Groups[1].Value.Trim())
            $dense = Join-Path ([IO.Path]::GetTempPath()) $name
            if ($name -match '^masonry-\d+-dense\.log$' -and (Test-Path -LiteralPath $dense -PathType Leaf)) {
                Export-DiagnosticText $dense 'masonry-dense.log'
            } else { $diagnostics.notes.Add('The referenced Masonry dense log was not found in the process temp directory.') }
        } else { $diagnostics.notes.Add('Native stderr did not contain a Masonry dense log pointer.') }

        # WER/event publication is asynchronous. Wait at most 30 seconds after an abnormal exit.
        # A WER policy, debugger, or fail-fast path may still prevent dump/event production.
        $abnormal = $forced -or ($app.HasExited -and $app.ExitCode -ne 0)
        $collectionDeadline = (Get-Date).AddSeconds($(if ($abnormal) { 30 } else { 0 }))
        $previousDumpSize = -1L
        do {
            $eventCount = Export-NativeEvents $nativeStarted $app.Id
            $dump = Get-Item -LiteralPath "$scratch/runebender.exe.$($app.Id).dmp" -ErrorAction SilentlyContinue
            $dumpReady = $false
            if ($dump -and $dump.Length -eq $previousDumpSize -and $dump.Length -gt 0) {
                # An exclusive open proves WER has closed the dump, not merely paused writing.
                try {
                    $handle = [IO.File]::Open($dump.FullName, 'Open', 'Read', 'None')
                    $handle.Dispose()
                    $dumpReady = $true
                } catch { }
            }
            if ($dump) { $previousDumpSize = $dump.Length }
            if (!$abnormal -or ($eventCount -gt 0 -and (!$diagnostics.dumpEnabled -or $dumpReady))) { break }
            Start-Sleep -Milliseconds 500
        } while ((Get-Date) -lt $collectionDeadline)
        if ($diagnostics.dumpEnabled -and $dumpReady) {
            if ($cdb) {
                try { Export-DumpAnalysis $cdb $dump.FullName } catch {
                    $diagnostics.notes.Add("Dump analysis failed: $_")
                    Write-Warning 'Dump analysis failed; partial output, when available, is retained.'
                }
            }
            # Binary memory cannot be redacted. This separate opt-in artifact has a hard size cap.
            if ($dump.Length -le 64MB) {
                $dumpOut = New-Item -ItemType Directory -Force 'windows-smoke-dumps'
                Copy-Item -LiteralPath $dump.FullName -Destination "$dumpOut/runebender.dmp"
                $diagnostics.dumpBytes = $dump.Length
                $diagnostics.dumpSha256 = (Get-FileHash "$dumpOut/runebender.dmp" -Algorithm SHA256).Hash
            } else { $diagnostics.notes.Add("Minidump exceeded the 64 MiB publication limit ($($dump.Length) bytes); not uploaded.") }
        } elseif ($diagnostics.dumpEnabled) { $diagnostics.notes.Add('No completed minidump arrived within the bounded collection window.') }
    }
} finally {
    # Restore only the values we changed, including their original registry types.
    if ($registry) {
        try {
            if ($registryTouched) {
                foreach ($name in @('DumpFolder', 'DumpType', 'DumpCount')) {
                    if ($oldValues.ContainsKey($name)) {
                        $registry.SetValue($name, $oldValues[$name].value, $oldValues[$name].kind)
                    } else { $registry.DeleteValue($name, $false) }
                }
            }
            $registry.Dispose()
            $registry = $null
            if ($registryCreated) { [Microsoft.Win32.Registry]::LocalMachine.DeleteSubKey($registryPath, $false) }
            $diagnostics.registryRestored = $true
        } catch {
            $diagnostics.registryRestored = $false
            $diagnostics.notes.Add("WER registry restoration failed: $_")
            Write-Warning 'WER registry restoration failed. See diagnostic-status.json.'
        }
    }
    if ($app) { $app.Dispose() }
    Protect-DiagnosticText ($diagnostics | ConvertTo-Json -Depth 6) | Set-Content "$out/diagnostic-status.json"
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
