# Capturing the vendor protocol

QuietLinux learns the keyboard's protocol by watching what the vendor software
sends: change one setting in IO Center, record the USB traffic, compare. This
guide shows three ways to record it and how to decode the result.

| Method | Runs on | Effort | Sees |
|---|---|---|---|
| [A: WebHID logger](#a-io-center-web-with-the-webhid-logger) | Linux (or any OS) with Chrome/Chromium | low | what IO Center Web sends and receives |
| [B: usbmon](#b-usbmon-and-wireshark-on-linux) | Linux | medium | the complete USB traffic of IO Center Web |
| [C: USBPcap](#c-io-center-on-windows-with-usbpcap) | Windows | medium | the complete USB traffic of the IO Center app |

The Windows app may offer features the web app lacks (see the IO Center
reviews: some key bindings need the desktop app), so both are worth capturing.

## Rules

1. **Never update the firmware**, and never capture a firmware update. Decline
   every update prompt. With the Windows app, disconnect the network while
   capturing.
2. **One change per capture.** Write down what you changed, from which value to
   which. Small, labeled captures are far easier to decode than one long session.
3. **Do not type anything private while capturing.** The keyboard reports key
   presses, so they end up in the capture. Use the mouse in IO Center.
4. **Raw captures stay private.** They contain the keyboard's serial number and
   whatever was typed. Keep them in `captures/` (ignored by Git) and share only
   the anonymized output of `qlcap fixtures` (see [Sharing](#sharing-results)).
5. **Note the state first.** An IO Center session may apply its own settings to
   the keyboard or reset key bindings. Note what the keyboard looks like before.

## Naming

```text
captures/<area>/<nr>-<what changed>.<ext>
captures/lighting/03-breathing-speed-50-to-100.pcapng
captures/lighting/03-breathing-speed-50-to-100.md     # your notes
```

Areas: `connect`, `lighting`, `keys`, `macros`, `profiles`, `settings`. The
list of things to capture is in [todo.md](todo.md#5-phase-2--data-collection).

## A: IO Center Web with the WebHID logger

The easiest method. A small script logs every HID report the web page
exchanges with the keyboard, together with notes you add.

Requirements: Chrome or Chromium, the [udev rule](installation.md#permissions-udev-rule)
installed (WebHID uses the same vendor interface as QuietLinux).

1. Close everything else that talks to the keyboard (another IO Center tab,
   a QuietLinux command running in a loop).
2. Open IO Center Web (linked from be quiet!'s IO Center download page) in
   Chrome. **Do not connect the keyboard yet.**
3. Open the DevTools console: <kbd>F12</kbd> › *Console*. If Chrome refuses to
   paste, type `allow pasting` and press Enter first.
4. Open [`tools/webhid-logger.js`](../tools/webhid-logger.js), read it, copy
   all of it, paste it into the console and press Enter. It answers
   `QuietLinux WebHID logger active`.
5. Connect the keyboard in the web app and pick *be quiet! Light Mount* in
   Chrome's device dialog.
6. Before each change, add a note in the console, then make the change in the app:

   ```js
   qlNote("breathing speed 50 -> 100")
   ```

7. When done, save the log. It lands in your downloads folder as
   `webhid-log-<name>.txt`:

   ```js
   qlSave("breathing-speed")
   ```

   `qlClear()` empties the log, e.g. to start the next capture cleanly.
8. Move the file to `captures/<area>/` and decode it ([Decoding](#decoding)).

The logger lives only in that tab: reloading the page removes it (paste it
again). If the console says `HIDDevice is not defined`, the page runs inside a
frame: pick the frame of the app in the context menu at the top of the console.

## B: usbmon and Wireshark on Linux

Records the complete USB traffic, independently of the browser.

1. Find the bus of the keyboard:

   ```sh
   lsusb -d 373f:0002
   # Bus 001 Device 003: ID 373f:0002 be quiet! be quiet! Light Mount
   ```

   Bus `001` means the capture interface is `usbmon1`. The device number
   changes on every replug.
2. Load the kernel module and allow your user to read that bus until the next
   reboot:

   ```sh
   sudo modprobe usbmon
   sudo setfacl -m u:$USER:r /dev/usbmon1
   ```

3. Start the capture, make **one** change in IO Center Web, stop with <kbd>Ctrl</kbd>+<kbd>C</kbd>:

   ```sh
   tshark -i usbmon1 -w captures/lighting/03-breathing-speed-50-to-100.pcapng
   ```

   Or use Wireshark: double-click `usbmon1`, stop, *File › Save As…* (pcapng).
4. Decode it ([Decoding](#decoding)).

A bus capture also records the other devices on that bus (mouse, another
keyboard). `qlcap` ignores them, but the raw file contains them: one more reason
to keep raw captures private.

## C: IO Center on Windows with USBPcap

Records what the Windows IO Center app sends. A real Windows PC works better than
a virtual machine (no USB passthrough, no resets when handing the device over).

### Once: prepare the PC

1. Install **Wireshark** for Windows from <https://www.wireshark.org/download.html>
   (*Windows x64 Installer*). In the installer, **tick "Install USBPcap"**.
   Restart Windows afterwards.
2. Install **IO Center** from be quiet!'s website. Plug in the keyboard.
3. In IO Center, open the product settings and **note the firmware version**.
   Do not update.
4. Turn off **Dynamic Lighting**: *Settings › Personalization › Dynamic
   Lighting › Use Dynamic Lighting on my devices: Off*. Otherwise Windows
   controls the lighting instead of IO Center.

### Every capture session

1. **Disconnect the network** (airplane mode, or unplug the cable / turn off
   Wi-Fi), then start IO Center. Keep it offline while capturing, so it cannot
   download or push a firmware update.
2. Start Wireshark. The start page lists `USBPcap1`, `USBPcap2`, …, one per USB
   root hub. If none is listed, start Wireshark as administrator.
3. Find the hub of the keyboard: click the **gear icon** next to each `USBPcapN`.
   The dialog lists the devices attached to that hub. The one with
   *be quiet! Light Mount* (or `373f:0002`) is yours.
4. In that same dialog, **tick only the Light Mount** and untick
   *Capture from all devices connected* and *Capture from newly connected
   devices*. This keeps the other devices (and what you type on them) out of
   the capture. Click *Start*.
5. In IO Center, make **exactly one change**. Wait a second.
6. In Wireshark, click the red **stop** button, then *File › Save As…*:
   format *pcapng*, a name like `03-breathing-speed-50-to-100.pcapng`.
7. Write what you changed into a text file of the same name (`.md` or `.txt`).
8. Repeat from step 4 (*Capture › Start* reuses the settings) for the next change.

### Afterwards

Copy the `.pcapng` files and notes to the Linux machine (USB stick, network
share, …) into `captures/<area>/` of your QuietLinux checkout, then decode them.
Reconnect the Windows PC to the network only after closing IO Center.

## Decoding

`qlcap` reads all three formats (pcapng/pcap from usbmon or USBPcap, and WebHID
logs), finds the keyboard by its packet format, reassembles fragmented messages
and prints one message per line:

```console
$ cargo run --release -p qlcap -- frames captures/lighting/03-breathing-speed-50-to-100.pcapng
     0.000  →  03:01 GetDeviceInfo          req 04 s00
     0.912  ←  03:01 GetDeviceInfo          req 04 s00  02 00 01 01 00 00 03 02
     1.020  →  03:02 GetSerialNumber        req 05 s00
     1.874  ←  03:02 GetSerialNumber        req 05 s00  <serial number redacted>
  1530.441  →  10:06 SetLayerConfig         req 2f s01  00 00 00 0a 32 00 e6 30 00
  2210.007  ←  11:02 ActionFired            notify s01  5c 80 01 09 03
```

Columns: time in ms, direction (`→` to the keyboard, `←` from it), command and
its name (as IO Center names it, see [protocol.md](protocol.md)), request ID
(`notify` for notifications the keyboard sends on its own), session byte, a
status name if the keyboard reported an error, data.

- `--no-key-events` hides key presses (`11:02`).
- `--device BUS.ADDRESS` picks a device if several are found (`qlcap` says so).
- The serial number is always redacted.

To compare two captures, decode both and diff them:

```sh
diff <(qlcap frames a.pcapng --no-key-events | cut -c13-) \
     <(qlcap frames b.pcapng --no-key-events | cut -c13-)
```

(`cut` drops the time column; `qlcap` is `target/release/qlcap` after
`cargo build --release`.)

## Sharing results

To contribute findings, share decoded output or fixtures, never raw captures:

```sh
qlcap fixtures captures/lighting/03-breathing-speed-50-to-100.pcapng > breathing-speed.hex
```

`fixtures` leaves out the serial number reply and all key events. Read the
output before sharing it, and include your notes (what you changed, firmware
version from `quietlinux status`).
