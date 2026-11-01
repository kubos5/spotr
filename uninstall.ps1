<#
.SYNOPSIS
    Spotr uninstaller for Windows: undoes what install.ps1 did, using the
    manifest it wrote.

.DESCRIPTION
    irm https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.ps1 | iex
    .\uninstall.ps1

    Only what the installer added is removed: Rust and the C++ build tools that
    were already on the computer stay. Your settings are kept unless you ask for
    them to go.

    `irm | iex` can't pass options; set them as environment variables first:
    $env:SPOTR_YES = 1 (-Yes), $env:SPOTR_KEEP_DEPS = 1 (-KeepDeps),
    $env:SPOTR_PURGE = 1 (-Purge) or 0 (-KeepSettings).

    Copyright (C) Spotr contributors. Licensed under the GNU GPL v3 or later.
#>
param(
    # Don't ask for confirmation (keeps your settings unless -Purge).
    [switch]$Yes,
    # Only remove Spotr; keep the Rust and build tools the installer added.
    [switch]$KeepDeps,
    # Also delete Spotr's settings, saved login and cache.
    [switch]$Purge,
    # Keep Spotr's settings, saved login and cache.
    [switch]$KeepSettings,
    [switch]$Help
)

# Everything runs in its own scope, so `irm | iex` leaves your session as it was.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    if ($Help) {
        Write-Host @'
Spotr uninstaller for Windows

USAGE:
    irm https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.ps1 | iex
    .\uninstall.ps1 [OPTIONS]

OPTIONS:
    -Yes            Don't ask for confirmation (keeps your settings unless -Purge)
    -KeepDeps       Only remove Spotr; keep the Rust and build tools the installer added
    -Purge          Also delete Spotr's settings, saved login and cache
    -KeepSettings   Keep Spotr's settings, saved login and cache
    -Help           Show this help

With irm | iex, set $env:SPOTR_YES = 1, $env:SPOTR_KEEP_DEPS = 1 or
$env:SPOTR_PURGE = 1 / 0 first.
'@
        return
    }

    $AssumeYes = $Yes -or $env:SPOTR_YES -eq '1'
    $KeepDepsOn = $KeepDeps -or $env:SPOTR_KEEP_DEPS -eq '1'
    # 1: delete your data, 0: keep it, $null: ask.
    $PurgeData = if ($Purge -or $env:SPOTR_PURGE -eq '1') { 1 } elseif ($KeepSettings -or $env:SPOTR_PURGE -eq '0') { 0 } else { $null }
    $LocalApp = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $HOME 'AppData\Local' }
    $InstallDir = Join-Path $LocalApp 'spotr-install'
    $Manifest = Join-Path $InstallDir 'manifest'
    $Log = Join-Path ([IO.Path]::GetTempPath()) ('spotr-uninstall-' + [guid]::NewGuid().ToString('N').Substring(0, 8) + '.log')
    $S = @{ TotalW = 0; DoneW = 0 }   # no overall progress bar here

    # ------------------------------------------------------------- look & feel
    # Only the terminal's own 16 colors are used, so the installer follows your
    # theme, just like Spotr does. Glyphs are written as code points so this file
    # stays plain ASCII (Windows PowerShell misreads UTF-8 files without a BOM).

    function G([int]$code) { [string][char]$code }
    $esc = [char]27
    $vt = $false
    try { $vt = [bool]$Host.UI.SupportsVirtualTerminal } catch {}
    if ([Console]::IsOutputRedirected -or $env:NO_COLOR -or $env:TERM -eq 'dumb') { $vt = $false }
    $fancy = $vt
    if ($vt) {
        $cReset = "$esc[0m"; $cBold = "$esc[1m"; $cDim = "$esc[2m"
        $cAcc = "$esc[32m"; $cAccB = "$esc[1;32m"; $cBright = "$esc[1;92m"; $cMuted = "$esc[90m"
        $cRed = "$esc[31m"; $cYel = "$esc[33m"; $cGrn = "$esc[32m"; $cKey = "$esc[7m"
        $hide = "$esc[?25l"; $show = "$esc[?25h"; $clr = "$esc[2K"
        $palette = @(31, 33, 32, 36, 34, 35)
    } else {
        $cReset = ''; $cBold = ''; $cDim = ''; $cAcc = ''; $cAccB = ''; $cBright = ''; $cMuted = ''
        $cRed = ''; $cYel = ''; $cGrn = ''; $cKey = ''; $hide = ''; $show = ''; $clr = ''
        $palette = @()
    }
    # Windows Terminal (and most others) draw every glyph; the classic console's
    # fonts lack a few, so it gets close substitutes.
    $rich = $vt -and ($env:WT_SESSION -or $env:TERM_PROGRAM -or $env:ConEmuANSI -eq 'ON')
    $uni = $vt
    if ($uni) {
        $gTL = G 0x256D; $gTR = G 0x256E; $gBL = G 0x2570; $gBR = G 0x256F; $gH = G 0x2500; $gV = G 0x2502
        $gDot = G 0xB7; $gEll = G 0x2026; $gNote2 = G 0x266A
        $gWave = @(0x2581..0x2588 | ForEach-Object { G $_ })
        $gBarOn = G 0x2501; $gKnob = G 0x25CF; $gBarOff = G 0x2500; $gTo = G 0x2192
        if ($rich) {
            $gOk = G 0x2713; $gFail = G 0x2717; $gArrow = G 0x276F; $gNote = G 0x2139
            $gSpin = @(0x280B, 0x2819, 0x2839, 0x2838, 0x283C, 0x2834, 0x2826, 0x2827, 0x2807, 0x280F | ForEach-Object { G $_ })
        } else {
            $gOk = G 0x221A; $gFail = G 0xD7; $gArrow = '>'; $gNote = 'i'
            $gSpin = @('|', '/', '-', '\')
        }
    } else {
        $gTL = '+'; $gTR = '+'; $gBL = '+'; $gBR = '+'; $gH = '-'; $gV = '|'
        $gOk = 'ok'; $gFail = 'x'; $gArrow = '>'; $gDot = '-'; $gNote = 'i'; $gEll = '~'; $gNote2 = ''
        $gSpin = @('|', '/', '-', '\')
        $gWave = @('_', '.', '-', '~', '~', '^', '^', '^')
        $gBarOn = '='; $gKnob = '>'; $gBarOff = '-'; $gTo = '->'
    }

    $cols = 80
    try { $cols = [Console]::WindowWidth } catch {}
    if ($cols -le 0) { $cols = 80 }
    $W = if ($cols -gt 76) { 76 } else { $cols - 2 }
    if ($W -lt 40) { $W = 40 }
    # The banner is centered over the content (the panels below it), not the
    # whole terminal, so on a wide terminal it stays above the text.
    $Span = [Math]::Min($W + 4, $cols)

    $oldEncoding = $null
    try { $oldEncoding = [Console]::OutputEncoding; [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch {}

    function Out([string]$s) { [Console]::Write($s) }
    function Line([string]$s = '') { [Console]::WriteLine($s) }
    function Half([int]$n) { [int][Math]::Floor($n / 2) }
    # A string without its color codes, and its printed width.
    function Strip([string]$s) { $s -replace "$esc\[[0-9;?]*[A-Za-z]", '' }
    function VLen([string]$s) { (Strip $s).Length }
    # Cut a string to $n columns (dropping its colors if it has to be cut).
    function Fit([string]$s, [int]$n) {
        if ((VLen $s) -le $n) { return $s }
        $p = Strip $s
        return $p.Substring(0, [Math]::Max(0, $n - 1)) + $gEll
    }
    function Rep([string]$s, [int]$n) { if ($n -le 0) { return '' }; return $s * $n }
    function Center([string]$s) { Line ((' ' * [Math]::Max(0, (Half ($Span - (VLen $s))))) + $s) }
    function Short([string]$p) { if ($p.StartsWith($HOME, [StringComparison]::OrdinalIgnoreCase)) { '~' + $p.Substring($HOME.Length) } else { $p } }

    # Box 'Title' @(lines)   -- a rounded panel like Spotr's popups.
    function Box([string]$title, [string[]]$lines) {
        $inner = $W - 2
        if ($title) {
            $len = VLen $title
            Line (" $cMuted$gTL$gH $cAccB$title$cReset $cMuted" + (Rep $gH ($inner - $len - 3)) + "$gTR$cReset")
        } else {
            Line (" $cMuted$gTL" + (Rep $gH $inner) + "$gTR$cReset")
        }
        foreach ($l in $lines) {
            $l = Fit $l ($inner - 2)
            Line (" $cMuted$gV$cReset $l" + (' ' * [Math]::Max(0, $inner - 2 - (VLen $l))) + " $cMuted$gV$cReset")
        }
        Line (" $cMuted$gBL" + (Rep $gH $inner) + "$gBR$cReset")
    }

    # o = lower half block, # = full block, ^ = upper half block.
    $Logo = @(
        'o##### #####o o####o ###### #####o ',
        '^^^ooo ##oo#^ ##  ##   ##   ##oo##o',
        '#####^ ##     ^####^   ##   ##   ##'
    ) | ForEach-Object { $_.Replace('o', (G 0x2584)).Replace('#', (G 0x2588)).Replace('^', (G 0x2580)) }
    $Tagline = 'a better Spotify client, for the terminal'

    # The logo with a shimmer band sweeping across it (Spotr's title shimmer).
    function DrawLogo([int]$band) {
        $pad = ' ' * [Math]::Max(0, (Half ($Span - 35)))
        foreach ($row in $Logo) {
            $sb = [Text.StringBuilder]::new()
            for ($i = 0; $i -lt $row.Length; $i++) {
                $col = if ($band -ge 0 -and [Math]::Abs($i - $band) -le 2) { $cBright } else { $cAccB }
                [void]$sb.Append($col).Append($row[$i])
            }
            Line ($clr + $pad + $sb.ToString() + $cReset)
        }
    }

    $Sine = @(4, 5, 6, 7, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3)
    function Wave([int]$t) {
        # Anchored to the logo (35 wide), one column in from its top row.
        $pad = ' ' * [Math]::Max(0, (Half ($Span - 35)) + 1)
        $sb = [Text.StringBuilder]::new()
        for ($i = 0; $i -lt 32; $i++) {
            if ($palette.Count) { [void]$sb.Append("$esc[" + $palette[$i % $palette.Count] + 'm') }
            [void]$sb.Append($gWave[$Sine[($i + $t) % $Sine.Count]])
        }
        Line ($clr + $pad + $sb.ToString() + $cReset)
    }

    function Banner([string]$subtitle) {
        Line
        if ($fancy) {
            Out $hide
            $n = $Tagline.Length
            $pad = ' ' * [Math]::Max(0, (Half ($Span - $n)))
            # Shimmer + typewriter tagline + rippling wave, then settle.
            for ($f = 0; $f -le 44; $f++) {
                DrawLogo ($f - 4)
                Line
                Line ($clr + $pad + $cDim + $Tagline.Substring(0, [Math]::Min($n, [int][Math]::Floor($f * $n / 30))) + $cReset)
                Line
                Wave $f
                Start-Sleep -Milliseconds 20
                Out "$esc[7A"
            }
            DrawLogo -10
            Line; Center "$cDim$Tagline$cReset"; Line
            Wave 44
            Out $show
        } else {
            Center 'S P O T R'; Line; Center $Tagline
        }
        Line
        Center "$cMuted$subtitle$cReset"
        Line
    }

    # A Spotr-style progress bar, in per mille.
    function Bar([int]$pm) {
        $bw = $W - 12
        $pm = [Math]::Max(0, [Math]::Min(1000, $pm))
        $on = [int][Math]::Floor($pm * $bw / 1000)
        "    $cAcc" + (Rep $gBarOn $on) + "$cBright$gKnob$cMuted" + (Rep $gBarOff ($bw - $on)) + "$cReset $cBold" + ('{0,3}%' -f [int][Math]::Floor($pm / 10)) + $cReset
    }

    # A step shows a spinner, what it's doing and the overall progress bar; its
    # output goes to the log. WEIGHT is roughly how many seconds it usually takes.
    function StepBegin([string]$label, [int]$weight) {
        if ($fancy) { Out $hide } else { Line "  ... $label" }
        @{ Label = $label; Weight = $weight; I = 0; P = 0; Sw = [Diagnostics.Stopwatch]::StartNew() }
    }

    # How far a step without a progress count is, guessed from the time it usually takes.
    function Guess($st) {
        $el = $st.Sw.Elapsed.TotalSeconds
        [int]($el * 1000 / ($el + $st.Weight + 1))
    }

    function StepDraw($st, [string]$detail, [int]$pm) {
        if (-not $fancy) { return }
        $el = [int][Math]::Floor($st.Sw.Elapsed.TotalSeconds)
        $room = $W - (VLen $st.Label) - 12
        $d = if ($room -gt 0 -and $detail) { Fit $detail $room } else { '' }
        $spin = $gSpin[$st.I % $gSpin.Count]
        $st.I++
        $text = "`r$clr  $cAcc$spin$cReset $($st.Label)  $cMuted$d$cReset"
        if ($el -ge 5) { $text += " $cMuted$($el)s$cReset" }
        if ($S.TotalW -gt 0) {
            # Never move backwards (estimates hand over to exact counts).
            if ($pm -gt 990) { $pm = 990 }
            if ($pm -gt $st.P) { $st.P = $pm }
            $overall = [int](($S.DoneW * 1000 + $st.Weight * $st.P) / $S.TotalW)
            $text += "`n$clr" + (Bar $overall) + "$esc[1A"
        }
        Out $text
    }

    function StepEnd($st, [bool]$ok) {
        $el = [int][Math]::Floor($st.Sw.Elapsed.TotalSeconds)
        $S.DoneW += $st.Weight
        if ($ok) {
            $text = "`r$clr  $cGrn$gOk$cReset $($st.Label)"
            if ($el -ge 5) { $text += " $cMuted$gDot $($el)s$cReset" }
            Out ($text + "`n$clr")
            # Keep the bar under the finished steps until the next one redraws it.
            if ($fancy -and $S.TotalW -gt 0) { Out ((Bar ([int]($S.DoneW * 1000 / $S.TotalW))) + "`r") }
            Out $show
            return
        }
        Out "`r$clr  $cRed$gFail$cReset $($st.Label)`n$clr`n"
        Out $show
        $tail = @('')
        $logText = ''
        try { $logText = [IO.File]::ReadAllText($Log) } catch {}
        $logLines = @($logText -split "[`r`n]+" | Where-Object { $_ -and $_ -notmatch 'Building \[' })
        foreach ($l in ($logLines | Select-Object -Last 12)) { $tail += "$cMuted$(Fit $l ($W - 6))$cReset" }
        Box "${cRed}Something went wrong$cReset" ($tail + @('', "Full log: $cBold$Log$cReset"))
        Line
        throw 'spotr-abort'
    }

    # The end of a file another process is still writing.
    function Tail([string]$path, [int]$bytes) {
        try {
            $fs = [IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
            try {
                $n = [int][Math]::Min($bytes, $fs.Length)
                $null = $fs.Seek(-$n, 'End')
                $buf = New-Object byte[] $n
                $null = $fs.Read($buf, 0, $n)
                return [Text.Encoding]::UTF8.GetString($buf)
            } finally { $fs.Dispose() }
        } catch { return '' }
    }

    function Quote([string]$a) { if ($a -match '[\s"]') { '"' + $a.Replace('"', '\"') + '"' } else { $a } }

    # Start a program with its output going to files (merged into the log by Collect).
    function Launch([string]$exe, [string[]]$argv, [string]$cwd, [switch]$Wait) {
        $p = Start-Process -FilePath $exe -ArgumentList (($argv | ForEach-Object { Quote $_ }) -join ' ') -WorkingDirectory $cwd `
            -NoNewWindow -PassThru -Wait:$Wait -RedirectStandardOutput "$Log.out" -RedirectStandardError "$Log.err"
        $null = $p.Handle   # so ExitCode is kept
        $p
    }
    function Collect {
        foreach ($f in @("$Log.out", "$Log.err")) {
            if (Test-Path -LiteralPath $f) { Add-Content -LiteralPath $Log -Value ([IO.File]::ReadAllText($f)); Remove-Item -LiteralPath $f -Force }
        }
    }
    # Run a program to the end, its output going to the log. Returns its exit code.
    function Native([string]$exe, [string[]]$argv, [string]$cwd = (Get-Location).Path) {
        $p = Launch $exe $argv $cwd -Wait
        Collect
        $p.ExitCode
    }
    # What a program prints, ignoring its errors ($null if it can't run). Windows
    # PowerShell turns a program's error output into a failure under 'Stop'.
    function Capture([string]$exe, [string[]]$argv) {
        $ErrorActionPreference = 'Continue'
        try { & $exe @argv 2>$null } catch { $null }
    }

    # RunStep WEIGHT 'Label' program @(arguments) [folder]   -- runs a program as a step.
    # Shows the crate cargo is compiling, and cargo's own unit counter as progress.
    function RunStep([int]$weight, [string]$label, [string]$exe, [string[]]$argv, [string]$cwd = (Get-Location).Path) {
        $st = StepBegin $label $weight
        try {
            $p = Launch $exe $argv $cwd
        } catch {
            Add-Content -LiteralPath $Log -Value ($_ | Out-String)
            StepEnd $st $false
        }
        while (-not $p.HasExited) {
            if ($fancy) {
                $t = (Tail "$Log.err" 4000) + (Tail "$Log.out" 2000)
                $detail = ''
                $m = [regex]::Matches($t, '(Compiling|Downloaded|Installing|Downloading|Unpacking) [^ \r\n]+( v[0-9.]+)?')
                if ($m.Count) { $detail = $m[$m.Count - 1].Value }
                $pm = Guess $st
                $c = [regex]::Matches($t, 'Building \[[^\]]*\] (\d+)/(\d+)')
                if ($c.Count) {
                    $g = $c[$c.Count - 1].Groups
                    # Units are counted equally, but the last one (Spotr itself) is the
                    # biggest, so leave the final stretch of the bar for it.
                    if ([int]$g[2].Value -gt 0) { $pm = [int]([int]$g[1].Value * 900 / [int]$g[2].Value) }
                }
                StepDraw $st $detail $pm
            }
            Start-Sleep -Milliseconds 80
        }
        $p.WaitForExit()
        Collect
        StepEnd $st ($p.ExitCode -eq 0)
    }

    # DoStep WEIGHT 'Label' { work }   -- quick work done here, as a step.
    function DoStep([int]$weight, [string]$label, [scriptblock]$work) {
        $st = StepBegin $label $weight
        StepDraw $st '' 0
        $ok = $true
        try { & $work *>> $Log } catch { Add-Content -LiteralPath $Log -Value ($_ | Out-String); $ok = $false }
        StepEnd $st $ok
    }

    function Info([string]$s) { Line "`r$clr  $cAcc$gNote$cReset $s" }

    function Die([string[]]$lines) {
        Line
        Box "${cRed}Can't continue$cReset" $lines
        Line
        throw 'spotr-abort'
    }

    # Read one key (Enter = the default, Esc = no).
    function Ask([string]$prompt, [string]$default) {
        if ($AssumeYes) { return $true }
        if ([Console]::IsInputRedirected) {
            Die @('No terminal to ask for confirmation.', "Set $cBold`$env:SPOTR_YES = 1$cReset (or pass $cBold-Yes$cReset) to skip the questions.")
        }
        Out "  $cAccB$gArrow$cReset $prompt "
        $k = [Console]::ReadKey($true)
        $c = [string]$k.KeyChar
        if ($k.Key -eq 'Enter') { $c = $default } elseif ($k.Key -eq 'Escape' -or $c -eq 'q') { $c = 'n' }
        if ($c -eq 'y') { Line "${cAcc}yes$cReset"; return $true }
        Line "${cMuted}no$cReset"
        return $false
    }

    # Remove one folder from your PATH (as stored, so other entries keep their %VARIABLES%).
    function RemoveFromPath([string]$dir) {
        $key = Get-Item -LiteralPath 'HKCU:\Environment'
        $cur = [string]$key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
        $want = $dir.TrimEnd('\')
        $keep = @($cur -split ';' | Where-Object { $_ -and [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ne $want })
        Set-ItemProperty -LiteralPath 'HKCU:\Environment' -Name 'Path' -Value ($keep -join ';') -Type ExpandString
        # Setting any variable through .NET tells open programs the environment changed.
        [Environment]::SetEnvironmentVariable('SPOTR_INSTALLER', '1', 'User')
        [Environment]::SetEnvironmentVariable('SPOTR_INSTALLER', $null, 'User')
    }

    function Main {
        # ---------------------------------------------------------- what to undo

        $onWindows = ($PSVersionTable.PSVersion.Major -lt 6) -or $IsWindows
        if (-not $onWindows) {
            Die @('This uninstaller is for Windows.', "On macOS and Linux, run ${cBold}uninstall.sh$cReset instead (see the README).")
        }
        $entries = @()
        if (Test-Path -LiteralPath $Manifest) { $entries = @(Get-Content -LiteralPath $Manifest) }
        function Value([string]$k) { $e = $entries | Where-Object { $_.StartsWith("$k=") } | Select-Object -First 1; if ($e) { $e.Substring($k.Length + 1) } else { '' } }

        $CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
        $Bin = Value 'bin'
        if (-not $Bin) { $Bin = Join-Path (Join-Path $CargoHome 'bin') 'spotr.exe' }
        $BinDir = Split-Path -Parent $Bin
        $Rustup = (Value 'rustup') -eq '1'
        $Tools = (Value 'buildtools') -eq '1'
        if ($KeepDepsOn) { $Rustup = $false; $Tools = $false }
        $BuildCache = @((Value 'src'), (Value 'target')) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
        # The PATH entry the installer added goes when nothing else lives in that folder
        # (and rustup, if it's being removed, takes its own entry with it).
        $PathDir = Value 'path'
        $others = @()
        if ($PathDir -and (Test-Path -LiteralPath $PathDir)) {
            $others = @(Get-ChildItem -LiteralPath $PathDir -Force | Where-Object { $_.Name -ne 'spotr.exe' -and $_.Name -ne 'spotr.exe.old' })
        }
        $DropPath = $PathDir -and ($others.Count -eq 0 -or $Rustup)

        # Spotr knows where its own files live; ask it before it's gone.
        $ConfigDir = ''; $CacheDir = ''
        if (Test-Path -LiteralPath $Bin) {
            foreach ($l in @(Capture $Bin @('--print-paths'))) {
                if ("$l" -match '^config\s+(.+)$') { $ConfigDir = Split-Path -Parent $Matches[1] }
                elseif ("$l" -match '^cache\s+(.+)$') { $CacheDir = $Matches[1] }
            }
        }
        $HasData = ($ConfigDir -and (Test-Path -LiteralPath $ConfigDir)) -or ($CacheDir -and (Test-Path -LiteralPath $CacheDir))

        Banner 'uninstaller'

        if (-not $entries.Count -and -not (Test-Path -LiteralPath $Bin)) {
            Box 'Nothing to do' @("Spotr doesn't seem to be installed.", "${cMuted}No install manifest, and no $cReset$(Short $Bin)$cMuted.$cReset")
            Line
            return
        }

        $plan = @()
        if (Test-Path -LiteralPath $Bin) { $plan += "$cRed$gFail$cReset Spotr $cMuted($(Short $Bin))$cReset" }
        if ($DropPath -and -not $Rustup) { $plan += "$cRed$gFail$cReset $(Short $PathDir) $cMuted(from your PATH)$cReset" }
        if ($BuildCache.Count) { $plan += "$cRed$gFail$cReset Spotr's source and build cache $cMuted($(Short $InstallDir))$cReset" }
        if ($Rustup) { $plan += "$cRed$gFail$cReset Rust $cMuted(rustup, ~\.cargo, ~\.rustup and its PATH entry)$cReset" }
        if ($Tools) { $plan += "$cRed$gFail$cReset Microsoft C++ build tools $cMuted(the ones the installer added)$cReset" }
        if ($entries.Count) { $plan += "$cRed$gFail$cReset The install manifest $cMuted($(Short $Manifest))$cReset" }
        if (-not $entries.Count) {
            $plan += @('', "$cYel!$cReset ${cMuted}No install manifest, so only Spotr itself is removed.$cReset")
        } elseif ($KeepDepsOn) {
            $plan += @('', "${cMuted}Keeping the Rust and build tools the installer added (-KeepDeps).$cReset")
        }
        if ($HasData -and $null -eq $PurgeData) {
            $plan += @('', "${cMuted}You'll be asked separately about your settings and saved login.$cReset")
        }
        Box 'This will remove' $plan
        Line

        if (-not (Ask "Uninstall now? $cMuted[${cReset}Y$cMuted/n]$cReset" 'y')) {
            Line; Line "  ${cMuted}Nothing was changed.$cReset"; Line
            return
        }

        if ($null -eq $PurgeData -and $HasData) {
            if ($AssumeYes) {
                $PurgeData = 0
            } else {
                Line
                $where = @()
                if ($ConfigDir -and (Test-Path -LiteralPath $ConfigDir)) { $where += "${cMuted}settings & login  $cReset$(Short $ConfigDir)" }
                if ($CacheDir -and (Test-Path -LiteralPath $CacheDir)) { $where += "${cMuted}cache & logs      $cReset$(Short $CacheDir)" }
                Box 'Your Spotr data' $where
                Line
                $PurgeData = if (Ask "Delete it too? $cMuted[y/${cReset}N$cMuted]$cReset" 'n') { 1 } else { 0 }
            }
        }
        Line

        # ---------------------------------------------------------------- uninstall

        if (Test-Path -LiteralPath $Bin) {
            $running = @(Get-Process -Name 'spotr' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Bin })
            if ($running.Count) { Die @('Spotr is still running.', 'Quit it (Ctrl+Q), then run this again.') }
            DoStep 1 'Removing Spotr' {
                Remove-Item -LiteralPath $Bin -Force
                if (Test-Path -LiteralPath "$Bin.old") { Remove-Item -LiteralPath "$Bin.old" -Force }
            }
        }
        if ($DropPath -and -not $Rustup) {
            DoStep 1 "Removing $(Short $PathDir) from your PATH" { RemoveFromPath $PathDir }
        }
        if ($BuildCache.Count) {
            DoStep 1 'Removing the build cache' { foreach ($d in $BuildCache) { Remove-Item -LiteralPath $d -Recurse -Force } }
        }
        if ($PurgeData -eq 1) {
            DoStep 1 'Deleting your Spotr settings and cache' {
                foreach ($d in @($ConfigDir, $CacheDir)) { if ($d -and (Test-Path -LiteralPath $d)) { Remove-Item -LiteralPath $d -Recurse -Force } }
            }
        }
        if ($Rustup -and (Get-Command rustup -ErrorAction SilentlyContinue)) {
            RunStep 1 'Removing Rust' 'rustup' @('self', 'uninstall', '-y')
        }
        if ($Tools -and (Get-Command winget -ErrorAction SilentlyContinue)) {
            Info "Windows will ask to let the Visual Studio Installer make changes. Click ${cBold}Yes$cReset."
            RunStep 1 'Removing the C++ build tools' 'winget' @('uninstall', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '--exact', '--silent', '--accept-source-agreements')
        }
        if ($entries.Count) {
            DoStep 1 'Cleaning up' {
                Remove-Item -LiteralPath $Manifest -Force
                # Folders we created, if they're empty now (the deepest are listed first).
                $dirs = @($InstallDir) + @($entries | Where-Object { $_.StartsWith('dir=') } | ForEach-Object { $_.Substring(4) })
                foreach ($d in $dirs) {
                    if ((Test-Path -LiteralPath $d) -and -not (Get-ChildItem -LiteralPath $d -Force | Select-Object -First 1)) { Remove-Item -LiteralPath $d -Force }
                }
            }
        }
        Remove-Item -LiteralPath $Log -Force -ErrorAction SilentlyContinue

        # --------------------------------------------------------------------- done

        Line
        $final = @("$cGrn$gOk$cReset ${cBold}Spotr has been uninstalled.$cReset")
        if ($PurgeData -ne 1 -and $HasData) {
            $final += @('', "${cMuted}Your settings were kept, in case you come back:$cReset")
            if ($ConfigDir -and (Test-Path -LiteralPath $ConfigDir)) { $final += "  $(Short $ConfigDir)" }
            if ($CacheDir -and (Test-Path -LiteralPath $CacheDir)) { $final += "  $(Short $CacheDir)" }
        }
        if ($Rustup -or $DropPath) { $final += @('', "${cMuted}Open a new terminal so the old PATH is gone.$cReset") }
        $thanks = if ($uni) { "${cMuted}Thanks for trying Spotr $cAcc$gNote2$cReset" } else { "${cMuted}Thanks for trying Spotr.$cReset" }
        $final += @('', $thanks)
        Box 'Goodbye' $final
        Line
    }

    try {
        Main
    } catch {
        if ("$_" -ne 'spotr-abort') {
            Out "$show`r$clr"
            Line
            Box "${cRed}Something went wrong$cReset" @('', "$cMuted$_$cReset", '', "Log: $cBold$Log$cReset")
            Line
            try { Add-Content -LiteralPath $Log -Value ($_ | Out-String) } catch {}
        }
    } finally {
        Out $show
        if ($oldEncoding) { try { [Console]::OutputEncoding = $oldEncoding } catch {} }
    }
}
