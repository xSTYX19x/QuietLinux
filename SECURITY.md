# Security

QuietLinux talks to a keyboard, and the keyboard's vendor interface reports
every key press. Security problems therefore matter: for example a way to read
keystrokes through QuietLinux, to run commands through a key binding or a
backup file, or to make it send something to the keyboard it should not.

## Reporting a problem

Please report it **privately**: on GitHub, open the repository's **Security**
tab and choose **Report a vulnerability**
([direct link](https://github.com/xSTYX19x/QuietLinux/security/advisories/new)).
Do not open a public issue. Include the version (`quietlinux --version`), what
an attacker can do and how to reproduce it.

You will get an answer as soon as possible; this is a spare-time project, so
please allow a few days.

## Supported versions

Only the latest release gets fixes.

## What QuietLinux does to stay safe

- Runs without root. The udev rule gives the user logged in at the computer
  access to the keyboard's vendor and LampArray interfaces only.
- No network access and no open ports. Website keys are opened with
  `xdg-open` (`http://` and `https://` only, without a shell).
- Never sends firmware updates; writes only formats captured from the vendor's
  own software, and only to tested firmware versions.
- No `unsafe` code; dependencies are checked with `cargo deny` in CI.

Known limit: any program running as the logged-in user can read key presses
from the vendor interface once the udev rule is installed (see
[installation](docs/installation.md#permissions-udev-rule)).
