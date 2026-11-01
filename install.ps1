<#
.SYNOPSIS
    Spotr installer for Windows: downloads Spotr (or builds it from source) and
    puts it on your PATH.

.DESCRIPTION
    irm https://raw.githubusercontent.com/kubos5/spotr/main/install.ps1 | iex
    .\install.ps1                 from a clone of the repository (builds it from source)

    It always installs the latest release, so the command above never changes.
    Everything it adds is written to a manifest so uninstall.ps1 can undo it.

    `irm | iex` can't pass options; set them as environment variables first:
    $env:SPOTR_YES = 1 (-Yes), $env:SPOTR_FROM_SOURCE = 1 (-FromSource).

    Copyright (C) Spotr contributors. Licensed under the GNU GPL v3 or later.
#>
param(
    # Don't ask for confirmation.
    [switch]$Yes,
    # Always compile, even when a prebuilt release is available.
    [switch]$FromSource,
    [switch]$Help
)

# Everything runs in its own scope, so `irm | iex` leaves your session as it was.
& {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell's own progress bar makes downloads crawl.
    $ProgressPreference = 'SilentlyContinue'

    if ($Help) {
        Write-Host @'
Spotr installer for Windows

USAGE:
    irm https://raw.githubusercontent.com/kubos5/spotr/main/install.ps1 | iex
    .\install.ps1 [OPTIONS]        (from a clone: builds it from source)

OPTIONS:
    -Yes            Don't ask for confirmation
    -FromSource     Always compile, even when a prebuilt release is available
    -Help           Show this help

With irm | iex, set $env:SPOTR_YES = 1 or $env:SPOTR_FROM_SOURCE = 1 first.
Undo everything with uninstall.ps1
'@
        return
    }

    # Overridable for forks (and for testing the installer).
    $RepoUrl = if ($env:SPOTR_REPO) { $env:SPOTR_REPO } else { 'https://github.com/kubos5/spotr.git' }
    $Branch = if ($env:SPOTR_BRANCH) { $env:SPOTR_BRANCH } else { 'main' }
    $Releases = if ($env:SPOTR_RELEASES) { $env:SPOTR_RELEASES } else { 'https://github.com/kubos5/spotr/releases' }
    $MinRust = [version]'1.90'
    $Triple = 'x86_64-pc-windows-msvc'
    $AssumeYes = $Yes -or $env:SPOTR_YES -eq '1'
    $ForceSource = $FromSource -or $env:SPOTR_FROM_SOURCE -eq '1'
    $LocalApp = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $HOME 'AppData\Local' }
    # The manifest, and the source and build cache when compiling.
    $InstallDir = Join-Path $LocalApp 'spotr-install'
    $Manifest = Join-Path $InstallDir 'manifest'
    $Log = Join-Path ([IO.Path]::GetTempPath()) ('spotr-install-' + [guid]::NewGuid().ToString('N').Substring(0, 8) + '.log')
    $S = @{ TotalW = 0; DoneW = 0 }   # overall progress (step weights done / planned)

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

    try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}
    Add-Type -AssemblyName System.Net.Http
    $Http = [Net.Http.HttpClient]::new()
    $Http.Timeout = [TimeSpan]::FromMinutes(30)
    $null = $Http.DefaultRequestHeaders.UserAgent.TryParseAdd('spotr-installer')

    # DownloadStep WEIGHT 'Label' url file   -- with the download's own progress.
    function DownloadStep([int]$weight, [string]$label, [string]$url, [string]$dest) {
        $st = StepBegin $label $weight
        StepDraw $st '' 0
        $ok = $true
        try {
            Add-Content -LiteralPath $Log -Value "GET $url"
            $resp = $Http.GetAsync($url, [Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
            if (-not $resp.IsSuccessStatusCode) { throw "HTTP $([int]$resp.StatusCode) for $url" }
            $total = $resp.Content.Headers.ContentLength
            $in = $resp.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $file = [IO.File]::Create($dest)
            try {
                $buf = New-Object byte[] 65536
                $got = 0; $last = -1000
                while (($n = $in.Read($buf, 0, $buf.Length)) -gt 0) {
                    $file.Write($buf, 0, $n)
                    $got += $n
                    if ($st.Sw.ElapsedMilliseconds - $last -ge 80) {
                        $last = $st.Sw.ElapsedMilliseconds
                        $mb = '{0:0.0} MB' -f ($got / 1MB)
                        if ($total) { $mb += ' of {0:0.0} MB' -f ($total / 1MB); $pm = [int]($got * 1000 / $total) } else { $pm = Guess $st }
                        StepDraw $st $mb $pm
                    }
                }
            } finally { $file.Dispose(); $in.Dispose() }
        } catch { Add-Content -LiteralPath $Log -Value ($_ | Out-String); $ok = $false }
        StepEnd $st $ok
    }

    # Where a URL ends up after redirects ($null if it isn't there).
    function Probe([string]$url) {
        try {
            $req = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Head, $url)
            $cts = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(8))
            $resp = $Http.SendAsync($req, $cts.Token).GetAwaiter().GetResult()
            if ($resp.IsSuccessStatusCode) { return $resp.RequestMessage.RequestUri.AbsoluteUri }
        } catch {}
        return $null
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

    function Main {
        # ------------------------------------------------------- detection

        $onWindows = ($PSVersionTable.PSVersion.Major -lt 6) -or $IsWindows
        if (-not $onWindows) {
            Die @('This installer is for Windows.', "On macOS and Linux, run ${cBold}install.sh$cReset instead (see the README).")
        }
        $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }

        $CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
        $BinDir = Join-Path $CargoHome 'bin'
        $Bin = Join-Path $BinDir 'spotr.exe'
        # Rust may be installed but not on this session's PATH yet.
        if ((Test-Path -LiteralPath (Join-Path $BinDir 'cargo.exe')) -and -not (($env:Path -split ';') -contains $BinDir)) {
            $env:Path = "$BinDir;$env:Path"
        }

        # Source: this clone (built in its own target folder), or a copy kept in
        # the install folder. Either way, builds are incremental: on later runs
        # (updates), only what changed is compiled again.
        $InRepo = $false
        $here = $PSScriptRoot
        if ($here -and (Test-Path -LiteralPath (Join-Path $here 'Cargo.toml')) -and
            (Select-String -LiteralPath (Join-Path $here 'Cargo.toml') -Pattern '^name = "spotr"' -Quiet)) {
            $InRepo = $true; $Src = $here; $TargetDir = Join-Path $here 'target'
        } else {
            $Src = Join-Path $InstallDir 'src'; $TargetDir = Join-Path $InstallDir 'target'
        }
        $Version = ''
        $toml = Join-Path $Src 'Cargo.toml'
        if (Test-Path -LiteralPath $toml) {
            $m = Select-String -LiteralPath $toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1
            if ($m) { $Version = $m.Matches[0].Groups[1].Value }
        }
        $Warm = Test-Path -LiteralPath (Join-Path $TargetDir 'local\deps')

        # The version installed now, if any.
        $Installed = ''
        if (Test-Path -LiteralPath $Bin) {
            $Installed = ([string](Capture $Bin @('--version')) -split ' ')[1]
        }

        # Prebuilt binaries (from GitHub releases) skip compiling altogether. Used
        # when installing straight from GitHub; a clone is always built from its source.
        $Prebuilt = ''
        if (-not $InRepo -and -not $ForceSource -and ($arch -eq 'AMD64' -or $arch -eq 'ARM64')) {
            $url = "$Releases/latest/download/spotr-$Triple.zip"
            if (Probe $url) {
                $Prebuilt = $url
                $tag = Probe "$Releases/latest"
                if ($tag) { $Version = ($tag -split '/')[-1].TrimStart('v') }
            }
        }

        # What compiling needs: Rust, and the Microsoft C++ build tools it links with.
        # (Nothing, for a prebuilt download.)
        $RustState = if ($Prebuilt) { 'skip' } else { 'missing' }
        $have = ''
        $NeedTools = $false; $HaveWinget = $false; $NeedGit = $false
        if (-not $Prebuilt) {
            if (Get-Command cargo -ErrorAction SilentlyContinue) {
                $have = ([string](Capture 'rustc' @('--version')) -split ' ')[1]
                $hv = $null
                if ($have -match '^(\d+\.\d+(\.\d+)?)') { $hv = [version]$Matches[1] }
                if ($hv -and $hv -ge $MinRust) { $RustState = 'ok' }
                elseif (Get-Command rustup -ErrorAction SilentlyContinue) { $RustState = 'update' }
                else { $RustState = 'old' }
            }
            # Rust for ARM64 Windows links with the ARM64 tools.
            $component = if ($arch -eq 'ARM64') { 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' } else { 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' }
            $pf86 = if (${env:ProgramFiles(x86)}) { ${env:ProgramFiles(x86)} } else { $env:ProgramFiles }
            $vswhere = if ($pf86) { Join-Path $pf86 'Microsoft Visual Studio\Installer\vswhere.exe' } else { '' }
            $tools = $null
            if ($vswhere -and (Test-Path -LiteralPath $vswhere)) {
                $tools = Capture $vswhere @('-latest', '-products', '*', '-requires', $component, '-property', 'installationPath')
            }
            $NeedTools = -not $tools
            $HaveWinget = [bool](Get-Command winget -ErrorAction SilentlyContinue)
            # Without git, the source comes as a zip.
            $NeedGit = -not $InRepo -and -not (Get-Command git -ErrorAction SilentlyContinue)
        }

        $PrevInstall = Test-Path -LiteralPath $Manifest
        $paths = [string](Get-Item -LiteralPath 'HKCU:\Environment').GetValue('Path', '', 'DoNotExpandEnvironmentNames') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine')
        $OnPath = @($paths -split ';' | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') }) -contains $BinDir.TrimEnd('\')

        # --------------------------------------------------------- the plan

        $sub = 'installer'
        if ($Version) { $sub += " $gDot v$Version" }
        Banner $sub

        if (-not $Prebuilt -and $arch -ne 'AMD64' -and $arch -ne 'ARM64' -and $arch -ne 'x86') {
            Die @("Spotr doesn't support this processor ($arch).")
        }

        # Step weights ~ typical seconds, for the progress bar.
        $wTools = 600; $wRustup = 3; $wRust = 60; $wRustUpdate = 30; $wSrc = 10; $wDownload = 8; $wUnpack = 1; $wInstall = 1; $wPath = 1
        $wBuild = if ($Warm) { 20 } else { 240 }

        $plan = @()
        $verLabel = if ($Version) { " $Version" } else { '' }
        if ($Prebuilt) {
            if ($Installed -and $Version -and $Installed -ne $Version) {
                $plan += "$cAcc$gArrow$cReset Update Spotr $Installed $gTo $Version $cMuted(prebuilt for $Triple)$cReset"
            } elseif ($Installed -and $Installed -eq $Version) {
                $plan += "$cAcc$gArrow$cReset Reinstall Spotr $Version $cMuted(it's up to date)$cReset"
            } else {
                $plan += "$cAcc$gArrow$cReset Download Spotr$verLabel $cMuted(prebuilt for $Triple)$cReset"
            }
            if ($arch -eq 'ARM64') { $plan += "    ${cDim}runs through Windows' x64 emulation$cReset" }
            $S.TotalW = $wDownload + $wUnpack + $wInstall
        } else {
            $S.TotalW = $wBuild + $wInstall
            if ($NeedTools) {
                if (-not $HaveWinget) {
                    Die @('Compiling Spotr needs the Microsoft C++ build tools, and winget',
                        "isn't here to install them. Install ${cBold}Build Tools for Visual Studio$cReset",
                        '(pick "Desktop development with C++") from',
                        '  https://visualstudio.microsoft.com/visual-cpp-build-tools/',
                        'then run this again.')
                }
                $plan += "$cAcc$gArrow$cReset Microsoft C++ build tools $cMuted(via winget; a large download)$cReset"
                $S.TotalW += $wTools
            }
            switch ($RustState) {
                'missing' { $plan += "$cAcc$gArrow$cReset Rust $cMuted(via rustup, into ~\.cargo and ~\.rustup)$cReset"; $S.TotalW += $wRustup + $wRust }
                'update' { $plan += "$cAcc$gArrow$cReset Update Rust to stable $cMuted(found $have, need $MinRust+)$cReset"; $S.TotalW += $wRustUpdate }
            }
            if (-not $InRepo) {
                if (Test-Path -LiteralPath (Join-Path $Src '.git')) {
                    $plan += "$cAcc$gArrow$cReset Update the source $cMuted(kept in $(Short $InstallDir))$cReset"
                } else {
                    $plan += "$cAcc$gArrow$cReset Download the source $cMuted(kept, so updates are quick)$cReset"
                }
                $S.TotalW += $wSrc
                if ($NeedGit) { $S.TotalW += $wUnpack }
            }
            $verb = if ($PrevInstall) { 'Rebuild' } else { 'Build' }
            if ($Warm) { $plan += "$cAcc$gArrow$cReset $verb Spotr $cMuted(only what changed is compiled)$cReset" }
            else { $plan += "$cAcc$gArrow$cReset $verb Spotr $cMuted(a few minutes the first time)$cReset" }
        }
        $plan += "$cAcc$gArrow$cReset Install it as $cMuted$(Short $Bin)$cReset"
        if (-not $OnPath) {
            $plan += "$cAcc$gArrow$cReset Add $cMuted$(Short $BinDir)$cReset to your PATH"
            $S.TotalW += $wPath
        }
        $already = @()
        if (-not $Prebuilt -and -not $NeedTools) { $already += 'C++ build tools' }
        if ($RustState -eq 'ok') { $already += "Rust $have" }

        $lines = $plan
        if ($already.Count) { $lines += @('', "${cMuted}Already there: $($already -join ', ')$cReset") }
        $lines += @('', "${cMuted}Everything added is recorded, so ${cReset}uninstall.ps1$cMuted can undo it.$cReset")
        Box "Here's the plan" $lines
        Line

        if ($RustState -eq 'old') {
            Die @("Found Rust $have, but Spotr needs $MinRust or newer, and rustup isn't installed.",
                'Update Rust (or remove it so rustup can be used), then try again.')
        }

        if (-not (Ask "Install now? $cMuted[${cReset}Y$cMuted/n]$cReset" 'y')) {
            Line; Line "  ${cMuted}Nothing was changed.$cReset"; Line
            return
        }
        Line

        # ------------------------------------------------------- manifest
        # Written as we go, so even a failed install can be undone.

        $newDirs = @()
        $d = $InstallDir
        while ($d -and -not (Test-Path -LiteralPath $d)) { $newDirs += $d; $d = Split-Path -Parent $d }
        $null = New-Item -ItemType Directory -Force -Path $InstallDir
        if (-not $PrevInstall) {
            $head = @("# Spotr install manifest - read by uninstall.ps1. Don't edit.", "created=$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')", 'os=Windows')
            # Folders we created (removed again if empty).
            $head += @($newDirs | ForEach-Object { "dir=$_" })
            Set-Content -LiteralPath $Manifest -Value $head -Encoding UTF8
        }
        function Record([string]$entry) {
            if (@(Get-Content -LiteralPath $Manifest) -notcontains $entry) { Add-Content -LiteralPath $Manifest -Value $entry -Encoding UTF8 }
        }
        # New-Item -Force that records each folder it creates (deepest first), for uninstall.
        function MakeDir([string]$dir) {
            $made = @(); $x = $dir
            while ($x -and -not (Test-Path -LiteralPath $x)) { $made += $x; $x = Split-Path -Parent $x }
            $null = New-Item -ItemType Directory -Force -Path $dir
            foreach ($m in $made) { Record "dir=$m" }
        }

        # -------------------------------------------------------- install

        if (-not $Prebuilt -and $NeedTools) {
            Info "Windows will ask to let the Visual Studio Installer make changes. Click ${cBold}Yes$cReset."
            Record 'buildtools=1'
            RunStep $wTools 'Installing the C++ build tools' 'winget' @('install', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '--exact',
                '--accept-package-agreements', '--accept-source-agreements', '--override',
                "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --add $component --includeRecommended")
        }

        switch ($RustState) {
            'missing' {
                $host_ = switch ($arch) { 'ARM64' { 'aarch64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } default { 'x86_64-pc-windows-msvc' } }
                $init = Join-Path ([IO.Path]::GetTempPath()) 'rustup-init.exe'
                DownloadStep $wRustup 'Downloading rustup' "https://static.rust-lang.org/rustup/dist/$host_/rustup-init.exe" $init
                Record 'rustup=1'
                RunStep $wRust 'Installing Rust' $init @('-y', '--profile', 'minimal', '--default-toolchain', 'stable')
                Remove-Item -LiteralPath $init -Force -ErrorAction SilentlyContinue
                if (-not (($env:Path -split ';') -contains $BinDir)) { $env:Path = "$BinDir;$env:Path" }
            }
            'update' {
                RunStep $wRustUpdate 'Updating Rust' 'rustup' @('update', 'stable')
                $null = Native 'rustup' @('default', 'stable')
            }
        }

        MakeDir $BinDir
        Record "bin=$Bin"

        if ($Prebuilt) {
            $dl = Join-Path ([IO.Path]::GetTempPath()) ('spotr-dl-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
            $null = New-Item -ItemType Directory -Force -Path $dl
            $zip = Join-Path $dl 'spotr.zip'
            DownloadStep $wDownload "Downloading Spotr$verLabel" $Prebuilt $zip
            DoStep $wUnpack 'Checking and unpacking it' {
                # Check the archive against the checksum published with the release.
                $sum = $null
                try { $sum = $Http.GetStringAsync("$Prebuilt.sha256").GetAwaiter().GetResult() } catch {}
                if ($sum) {
                    $want = ($sum.Trim() -split '\s+')[0].ToLower()
                    $got = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
                    if ($want -ne $got) { throw "checksum mismatch: expected $want, got $got" }
                }
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                [IO.Compression.ZipFile]::ExtractToDirectory($zip, (Join-Path $dl 'x'))
            }
            $found = Get-ChildItem -LiteralPath (Join-Path $dl 'x') -Recurse -Filter 'spotr.exe' | Select-Object -First 1
            if (-not $found) { Die @("The release archive didn't contain spotr.exe.", "Run again with $cBold-FromSource$cReset to compile it instead.") }
            $Built = $found.FullName
            $v = ([string](Capture $Built @('--version')) -split ' ')[1]
            if ($v) { $Version = $v }
        } else {
            if (-not $InRepo) {
                MakeDir $InstallDir
                Record "src=$Src"
                Record "target=$TargetDir"
                if (Test-Path -LiteralPath (Join-Path $Src '.git')) {
                    DoStep $wSrc 'Updating the source' {
                        if (Native 'git' @('-C', $Src, 'fetch', '--depth', '1', 'origin', $Branch)) { throw 'git fetch failed' }
                        if (Native 'git' @('-C', $Src, 'reset', '--hard', 'FETCH_HEAD')) { throw 'git reset failed' }
                    }
                } elseif (-not $NeedGit) {
                    if (Test-Path -LiteralPath $Src) { Remove-Item -LiteralPath $Src -Recurse -Force }
                    RunStep $wSrc 'Downloading the source' 'git' @('clone', '--depth', '1', '--branch', $Branch, $RepoUrl, $Src)
                } else {
                    $base = $RepoUrl -replace '\.git$', ''
                    $zip = Join-Path $InstallDir 'source.zip'
                    DownloadStep $wSrc 'Downloading the source' "$base/archive/refs/heads/$Branch.zip" $zip
                    DoStep $wUnpack 'Unpacking the source' {
                        $tmp = Join-Path $InstallDir 'source-tmp'
                        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
                        Add-Type -AssemblyName System.IO.Compression.FileSystem
                        [IO.Compression.ZipFile]::ExtractToDirectory($zip, $tmp)
                        if (Test-Path -LiteralPath $Src) { Remove-Item -LiteralPath $Src -Recurse -Force }
                        Move-Item -LiteralPath (Get-ChildItem -LiteralPath $tmp -Directory | Select-Object -First 1).FullName -Destination $Src
                        Remove-Item -LiteralPath $tmp, $zip -Recurse -Force
                    }
                }
                $m = Select-String -LiteralPath (Join-Path $Src 'Cargo.toml') -Pattern '^version = "([^"]+)"' | Select-Object -First 1
                if ($m) { $Version = $m.Matches[0].Groups[1].Value; $verLabel = " $Version" }
            }
            # `cargo build` into a kept target folder (instead of `cargo install`, which
            # starts from scratch every time), with the quick-to-rebuild `local` profile
            # (see Cargo.toml) and cargo's progress counter in the log.
            $env:CARGO_TERM_PROGRESS_WHEN = 'always'; $env:CARGO_TERM_PROGRESS_WIDTH = '100'
            try {
                RunStep $wBuild "Building Spotr$verLabel" 'cargo' @('build', '--profile', 'local', '--locked', '--target-dir', $TargetDir) $Src
            } finally {
                Remove-Item Env:CARGO_TERM_PROGRESS_WHEN, Env:CARGO_TERM_PROGRESS_WIDTH -ErrorAction SilentlyContinue
            }
            $Built = Join-Path $TargetDir 'local\spotr.exe'
        }

        # A running spotr.exe can't be replaced, but it can be renamed out of the way.
        DoStep $wInstall "Installing to $(Short $Bin)" {
            $old = "$Bin.old"
            if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
            if (Test-Path -LiteralPath $Bin) { Move-Item -LiteralPath $Bin -Destination $old -Force }
            Copy-Item -LiteralPath $Built -Destination $Bin -Force
            if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
        }
        if ($Prebuilt) { Remove-Item -LiteralPath $dl -Recurse -Force -ErrorAction SilentlyContinue }

        if (-not $OnPath) {
            Record "path=$BinDir"
            DoStep $wPath "Adding $(Short $BinDir) to your PATH" {
                $key = Get-Item -LiteralPath 'HKCU:\Environment'
                $cur = [string]$key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
                $new = if ($cur) { "$BinDir;$cur" } else { $BinDir }
                Set-ItemProperty -LiteralPath 'HKCU:\Environment' -Name 'Path' -Value $new -Type ExpandString
                # Setting any variable through .NET tells open programs the environment changed.
                [Environment]::SetEnvironmentVariable('SPOTR_INSTALLER', '1', 'User')
                [Environment]::SetEnvironmentVariable('SPOTR_INSTALLER', $null, 'User')
            }
        }
        # This window can use it right away.
        if (-not (($env:Path -split ';') -contains $BinDir)) { $env:Path = "$BinDir;$env:Path" }
        if ($fancy) { Out "`r$clr" }
        Remove-Item -LiteralPath $Log -Force -ErrorAction SilentlyContinue

        # ----------------------------------------------------------- done

        Line
        $final = @("$cGrn$gOk$cReset ${cBold}Spotr$(if ($Version) { " $Version" }) is installed.$cReset", '')
        $final += "Start it with  $cKey spotr $cReset"
        if (-not $OnPath -or $RustState -eq 'missing') {
            $final += "${cMuted}(other open terminals need restarting first)$cReset"
        }
        $final += @('', "${cMuted}Just looking? Try ${cReset}spotr --demo$cMuted, no account needed.$cReset")
        if ($InRepo) {
            $final += "${cMuted}Changed your mind? ${cReset}.\uninstall.ps1$cMuted undoes all of this.$cReset"
            Box 'All set' $final
        } else {
            $final += "${cMuted}Changed your mind? This undoes all of it:$cReset"
            Box 'All set' $final
            Line "  irm https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.ps1 | iex"
        }
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
        $Http.Dispose()
    }
}
