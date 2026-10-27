# QuietLinux: record what IO Center sends to the be quiet! Light Mount, on Windows.
#
# Guides you through the captures listed below, one by one: Enter starts a
# capture, you make the change in IO Center, Enter stops and saves it. It uses
# USBPcapCMD.exe (installed with Wireshark's "Install USBPcap" option) and
# records every USB root hub; qlcap finds the keyboard in the files later.
# See docs/capturing.md.
#
# Run it in a PowerShell started as administrator:
#   powershell -ExecutionPolicy Bypass -File .\windows-capture.ps1
#
# Options:
#   -OutDir <folder>          where to save (default: quietlinux-captures on the Desktop)
#   -Only <name>,<name>,...   run only these steps, e.g. -Only win-polling,win-effects
#   -Hubs <hub>,<hub>,...     USBPcap interfaces to record, e.g. -Hubs \\.\USBPcap1
#                             (default: the ones USBPcapCMD lists, else USBPcap1-10 are tried)
#
# Privacy: the traffic of every USB device of this PC is recorded, including
# what is typed on any keyboard. Use only the mouse while a capture runs. The
# files stay on this PC until you copy them.
#
# Safety: disconnect the network before starting IO Center, never accept a
# firmware update, and do not use the firmware or factory reset pages.

#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$OutDir = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'quietlinux-captures'),
    [string[]]$Only = @(),
    [string[]]$Hubs = @()
)

$ErrorActionPreference = 'Stop'

$Steps = @(
    @{ Name = 'win-connect';       Text = 'IO Center must be closed completely (also its tray icon) before you press Enter. Then start IO Center, open the Light Mount, change nothing; wait 5 seconds.' },
    @{ Name = 'win-polling';       Text = 'Set the polling rate to 500 Hz, then back to 1000 Hz.' },
    @{ Name = 'win-effects';       Text = 'Select one after the other: Color Wave, Tornado, Reactive, Matrix (and Ripple, Off if offered).' },
    @{ Name = 'win-custom-mode';   Text = 'Per-key lighting: make W red and A green, then apply.' },
    @{ Name = 'win-custom-off';    Text = 'Go back to a normal effect.' },
    @{ Name = 'win-macro-create';  Text = 'Create a macro "test": A, a 100 ms delay, B. Save it; do not assign it yet.' },
    @{ Name = 'win-macro-assign';  Text = 'Assign the macro "test" to M3.' },
    @{ Name = 'win-macro-edit';    Text = 'Edit the macro "test": add C at the end. Save it.' },
    @{ Name = 'win-macro-repeat';  Text = 'Change the repeat mode of the macro "test" (e.g. 3 times, or while pressed).' },
    @{ Name = 'win-macro-delete';  Text = 'Delete the macro "test". Set M3 back to Next Track.' },
    @{ Name = 'win-bind-website';  Text = 'Bind M2 to "open website" (https://example.org), then M2 back to F13.' },
    @{ Name = 'win-bind-profile';  Text = 'Bind M2 to a profile switch, then M2 back to F13.' },
    @{ Name = 'win-profiles';      Text = 'Create profile 2, switch to it, pick another effect there, switch back to profile 1.' },
    @{ Name = 'win-gamemode';      Text = 'Switch game mode on in IO Center, then off.' },
    @{ Name = 'win-quicktap';      Text = 'Quick tap: add a key pair (e.g. A/D), then remove it.' },
    @{ Name = 'win-calibration';   Text = 'Change the LED color calibration, then set it back.' }
)

function Find-UsbPcap {
    $dirs = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) | Where-Object { $_ }
    foreach ($dir in $dirs) {
        foreach ($sub in @('USBPcap', 'Wireshark\extcap')) {
            $exe = Join-Path $dir (Join-Path $sub 'USBPcapCMD.exe')
            if (Test-Path $exe) { return $exe }
        }
    }
    return $null
}

function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    # stdout and stderr as text; a native program writing to stderr must not stop the script
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        return @(& $Exe @Arguments 2>&1 | ForEach-Object { "$_" })
    }
    catch {
        return @("(failed: $_)")
    }
    finally {
        $ErrorActionPreference = $previous
    }
}

function Get-ListedHubs([string[]]$Lines) {
    # USBPcapCMD lists its capture interfaces like: interface {value=\\.\USBPcap1}{display=USBPcap1}
    $found = @()
    foreach ($line in $Lines) {
        if ($line -match '(\\\\\.\\USBPcap\d+)') { $found += $Matches[1] }
    }
    return @($found | Select-Object -Unique)
}

function Show-Diagnostics([string]$UsbPcap) {
    Write-Host ''
    Write-Host '--- Diagnostics (please send this to the QuietLinux maintainer) ---' -ForegroundColor Yellow
    Write-Host ('USBPcapCMD: {0}' -f $UsbPcap)
    Write-Host 'USBPcapCMD --extcap-interfaces printed:'
    $lines = Invoke-Native $UsbPcap @('--extcap-interfaces')
    if ($lines.Count -eq 0) { Write-Host '  (nothing)' } else { $lines | ForEach-Object { Write-Host ('  ' + $_) } }
    Write-Host 'sc.exe query USBPcap:'
    Invoke-Native 'sc.exe' @('query', 'USBPcap') | ForEach-Object { Write-Host ('  ' + $_) }
    Write-Host ('Windows: {0}' -f [Environment]::OSVersion.VersionString)
    Write-Host '--- end ---' -ForegroundColor Yellow
    Write-Host ''
    Write-Host 'If the USBPcap service is not RUNNING, Windows did not load the USBPcap driver.'
    Write-Host 'A common cause on Windows 11 is "Memory integrity": Windows Security > Device security >'
    Write-Host 'Core isolation details. Check also whether Wireshark lists USBPcap1, USBPcap2, ... interfaces.'
}

function Start-Capture([string]$UsbPcap, [string[]]$HubList, [string]$Dir, [string]$Name) {
    $captures = @()
    foreach ($hub in $HubList) {
        $hubName = $hub -replace '^\\\\\.\\', ''
        $file = Join-Path $Dir ('{0}-{1}.pcap' -f $Name, $hubName)
        $arguments = @('-d', $hub, '-o', ('"{0}"' -f $file), '-A')
        $proc = Start-Process -FilePath $UsbPcap -ArgumentList $arguments -WindowStyle Hidden -PassThru
        $captures += [PSCustomObject]@{ Hub = $hub; File = $file; Proc = $proc }
    }
    return $captures
}

function Stop-Capture($Captures) {
    # give USBPcapCMD a moment to write the last packets
    Start-Sleep -Milliseconds 700
    foreach ($c in $Captures) {
        if (-not $c.Proc.HasExited) { Stop-Process -Id $c.Proc.Id -Force -ErrorAction SilentlyContinue }
    }
}

# --- checks ---------------------------------------------------------------------

$UsbPcap = Find-UsbPcap
if (-not $UsbPcap) {
    Write-Host 'USBPcapCMD.exe not found. Install Wireshark and tick "Install USBPcap", then restart Windows.' -ForegroundColor Red
    exit 1
}
Write-Host ('USBPcap: {0}' -f $UsbPcap)

$probing = $false
if ($Hubs.Count -eq 0) {
    $Hubs = Get-ListedHubs (Invoke-Native $UsbPcap @('--extcap-interfaces'))
}
if ($Hubs.Count -eq 0) {
    # not listed: try the usual names; those that do not exist stop at once
    $Hubs = @(1..10 | ForEach-Object { '\\.\USBPcap{0}' -f $_ })
    $probing = $true
    Write-Host 'USBPcapCMD lists no root hubs; trying USBPcap1 to USBPcap10 with the first capture.' -ForegroundColor Yellow
}
else {
    Write-Host ('Root hubs: {0}' -f ($Hubs -join ', '))
}

$online = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' })
if ($online.Count -gt 0) {
    Write-Host ''
    Write-Host ('Network connected: {0}' -f (($online | ForEach-Object { $_.Name }) -join ', ')) -ForegroundColor Yellow
    Write-Host 'Disconnect it before starting IO Center, so that it cannot download or install a firmware update.' -ForegroundColor Yellow
    $answer = Read-Host 'Enter = continue anyway, q = quit'
    if ($answer -eq 'q') { exit 0 }
}

$selected = @($Steps | Where-Object { $Only.Count -eq 0 -or $Only -contains $_.Name })
if ($selected.Count -eq 0) {
    Write-Host ('No step named {0}. Steps: {1}' -f ($Only -join ', '), (($Steps | ForEach-Object { $_.Name }) -join ', ')) -ForegroundColor Red
    exit 1
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Write-Host ''
Write-Host ('Saving to {0}' -f $OutDir)
Write-Host 'Before you start: Windows Settings > Personalization > Dynamic Lighting must be off.'
Write-Host 'While a capture runs, use only the mouse.'

# --- captures -------------------------------------------------------------------

$running = @()
try {
    $i = 0
    foreach ($step in $selected) {
        $i++
        Write-Host ''
        Write-Host ('[{0}/{1}] {2}' -f $i, $selected.Count, $step.Name) -ForegroundColor Cyan
        Write-Host ('    {0}' -f $step.Text)
        $answer = Read-Host '    Enter = start the capture, s = skip, q = quit'
        if ($answer -eq 'q') { break }
        if ($answer -eq 's') { continue }

        $dir = Join-Path $OutDir $step.Name
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        Set-Content -Path (Join-Path $dir 'note.txt') -Value $step.Text
        $running = @(Start-Capture $UsbPcap $Hubs $dir $step.Name)
        Start-Sleep -Seconds 1

        $alive = @($running | Where-Object { -not $_.Proc.HasExited })
        foreach ($dead in @($running | Where-Object { $_.Proc.HasExited })) {
            Remove-Item -LiteralPath $dead.File -ErrorAction SilentlyContinue
        }
        if ($alive.Count -eq 0) {
            $codes = ($running | ForEach-Object { $_.Proc.ExitCode }) -join ', '
            Write-Host ('    USBPcapCMD could not record on any root hub (exit codes: {0}).' -f $codes) -ForegroundColor Red
            Show-Diagnostics $UsbPcap
            $running = @()
            break
        }
        if ($probing) {
            $Hubs = @($alive | ForEach-Object { $_.Hub })
            $probing = $false
            Write-Host ('    Root hubs found: {0}' -f ($Hubs -join ', '))
        }
        $running = $alive

        Write-Host '    Recording. Make the change in IO Center now (mouse only).' -ForegroundColor Green
        [void](Read-Host '    Enter = stop and save')
        Stop-Capture $running
        $running = @()
        Write-Host ('    Saved in {0}' -f $dir)
    }
}
finally {
    # also on Ctrl+C: never leave USBPcapCMD running in the background
    if ($running.Count -gt 0) { Stop-Capture $running }
}

Write-Host ''
Write-Host 'Done. Copy the folder to the Linux machine, into captures/ of your QuietLinux checkout, and decode e.g.:'
Write-Host ('  qlcap frames captures/{0}/win-connect/*.pcap' -f (Split-Path $OutDir -Leaf))
