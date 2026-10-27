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
    [string[]]$Only = @()
)

$ErrorActionPreference = 'Stop'

$Steps = @(
    @{ Name = 'win-connect';       Text = 'Start IO Center (or open the Light Mount in it). Change nothing; wait 5 seconds.' },
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
        $exe = Join-Path $dir 'USBPcap\USBPcapCMD.exe'
        if (Test-Path $exe) { return $exe }
    }
    return $null
}

function Get-RootHubs([string]$UsbPcap) {
    # USBPcapCMD lists its capture interfaces like: interface {value=\\.\USBPcap1}{display=USBPcap1}
    $lines = & $UsbPcap --extcap-interfaces 2>$null
    $hubs = @()
    foreach ($line in $lines) {
        if ($line -match 'interface \{value=([^}]+)\}') { $hubs += $Matches[1] }
    }
    return $hubs
}

function Start-Capture([string]$UsbPcap, [string[]]$Hubs, [string]$Dir, [string]$Name) {
    $procs = @()
    foreach ($hub in $Hubs) {
        $hubName = $hub -replace '^\\\\\.\\', ''
        $file = Join-Path $Dir ('{0}-{1}.pcap' -f $Name, $hubName)
        $arguments = @('-d', $hub, '-o', ('"{0}"' -f $file), '-A')
        $procs += Start-Process -FilePath $UsbPcap -ArgumentList $arguments -WindowStyle Hidden -PassThru
    }
    return $procs
}

function Stop-Capture($Procs) {
    # give USBPcapCMD a moment to write the last packets
    Start-Sleep -Milliseconds 700
    foreach ($p in $Procs) {
        if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    }
}

# --- checks ---------------------------------------------------------------------

$UsbPcap = Find-UsbPcap
if (-not $UsbPcap) {
    Write-Host 'USBPcapCMD.exe not found. Install Wireshark and tick "Install USBPcap", then restart Windows.' -ForegroundColor Red
    exit 1
}
$Hubs = @(Get-RootHubs $UsbPcap)
if ($Hubs.Count -eq 0) {
    Write-Host 'USBPcap lists no USB root hubs. Restart Windows after installing USBPcap.' -ForegroundColor Red
    exit 1
}
Write-Host ('USBPcap: {0}' -f $UsbPcap)
Write-Host ('Root hubs: {0}' -f ($Hubs -join ', '))

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
        $failed = @($running | Where-Object { $_.HasExited })
        if ($failed.Count -eq $running.Count) {
            $codes = ($failed | ForEach-Object { $_.ExitCode }) -join ', '
            Write-Host ('    USBPcapCMD stopped right away (exit codes: {0}). Is this PowerShell running as administrator?' -f $codes) -ForegroundColor Red
            break
        }

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
