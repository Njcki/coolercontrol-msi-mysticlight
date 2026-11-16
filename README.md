# MSI Mystic Light ARGB for CoolerControl

**Control the ARGB headers (JRAINBOW1 / JRAINBOW2) of MSI Mystic Light motherboards from [CoolerControl](https://gitlab.com/coolercontrol/coolercontrol) on Linux: fixed colours, off, and 23 hardware effects.**

![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)
![Rust](https://img.shields.io/badge/rust-%E2%89%A5%201.86-orange)
![CoolerControl](https://img.shields.io/badge/CoolerControl-%E2%89%A5%205.1-5b8def)
![Platform: Linux](https://img.shields.io/badge/platform-Linux-lightgrey)

CoolerControl (through liquidctl) does not support MSI Mystic Light, so anything plugged into
the motherboard's 5 V ARGB headers (an AIO pump ring, fans, LED strips) stays out of reach. This
native [CoolerControl Device Service plugin](https://docs.coolercontrol.org/), written in Rust,
exposes those headers as lighting channels, so you can set them on CoolerControl's lighting
page next to the rest of your devices.

## Contents

- [Features](#features)
- [Supported hardware](#supported-hardware)
- [Requirements](#requirements)
- [Build and install](#build-and-install)
- [Usage in CoolerControl](#usage-in-coolercontrol)
- [Lighting modes](#lighting-modes)
- [Command line tools](#command-line-tools)
- [Known limitations](#known-limitations)
- [Troubleshooting](#troubleshooting)
- [Protocol notes](#protocol-notes)
- [Disclaimer](#disclaimer)
- [Credits](#credits)
- [License](#license)

## Features

- **Native CoolerControl device** *MSI Mystic Light* with two lighting channels:
  `JRAINBOW1 (ARGB 5V)` and `JRAINBOW2 (ARGB 5V)`.
- **23 modes**: fixed colour, off and the board's built-in hardware effects (rainbow wave,
  breathing, meteor, fire, ...), with slow / medium / fast speeds.
- **Nothing is written to flash**: changes live in the controller's RAM only, so a cold boot
  always brings back your BIOS / MSI Center setting.
- **Minimal writes**: only the bytes of the selected header (plus the sync bits that keep it
  independent from the onboard zones) are changed; the LED count stored on the board is kept.
- **Automatic backup**: the state found at start-up is saved, and *Reset* in CoolerControl
  restores the channel to it.
- **Board allow-list**: the plugin refuses to drive any board it does not know, so it cannot
  send a mismatched packet layout to a different controller.
- **CLI tools** to dump, test, set and restore the controller state without CoolerControl.

## Supported hardware

| Board | Controller USB ID | Board code | Headers | Status |
| --- | --- | --- | --- | --- |
| MSI X670E GAMING PLUS WIFI (MS-7E16) | `0db0:0076` ("MSI MYSTIC LIGHT") | `7E16` | JRAINBOW1, JRAINBOW2 | Tested |

Many MSI boards share the USB ID `0db0:0076`; they are told apart by the first four
characters of the controller's USB serial number (the board code, e.g. `7E16` for MS-7E16),
which select the packet layout and zone set. Only boards in `SUPPORTED_VIRTUAL_PIDS`
(`src/hid.rs`) are driven. Other boards that use the same 185-byte layout with JRAINBOW1/2
can probably be added: check the board configuration in OpenRGB's
`MSIMotherboardController`, then open an issue or a pull request with the `--dump` output
(remove the serial number line before posting).

## Requirements

- Linux with `hidraw`
- [CoolerControl](https://gitlab.com/coolercontrol/coolercontrol) **5.1 or newer** (Device
  Service plugins)
- To build: Rust **1.86+** (edition 2024) and `protoc` (protobuf compiler)
  - Arch / CachyOS: `sudo pacman -S rust protobuf`
  - Debian / Ubuntu: `sudo apt install protobuf-compiler` + [rustup](https://rustup.rs)
  - Fedora: `sudo dnf install protobuf-compiler` + rustup
- Do not run OpenRGB or another RGB tool on the same controller at the same time.

## Build and install

```bash
git clone https://github.com/<you>/<this-repo>.git
cd <this-repo>
cargo build --release

# optional but recommended: check the board is detected and take a backup (read only)
sudo ./target/release/msi_mysticlight --dump

sudo ./install.sh
```

`install.sh` copies the binary and `manifest.toml` to
`/var/lib/coolercontrol/plugins/msi_mysticlight/`, applies the SELinux context when SELinux
is enabled and restarts `coolercontrold`, which then runs the plugin as the
`cc-plugin-msi_mysticlight` service.

**Upgrade**: pull, rebuild, run `sudo ./install.sh` again.

**Uninstall**: `sudo ./uninstall.sh` first copies the plugin's state backups to `./backups`,
then removes the plugin folder, its service unit and socket, and restarts CoolerControl.

> The plugin runs as root (`privileged = true` in `manifest.toml`), because sending HID
> feature reports to `/dev/hidraw*` requires it. No udev rule is needed.

## Usage in CoolerControl

1. Open CoolerControl; a device named **MSI Mystic Light** appears.
2. Select the `JRAINBOW1 (ARGB 5V)` or `JRAINBOW2 (ARGB 5V)` channel.
3. Pick a mode, a colour (where the mode takes one) and a speed, then apply.

Not sure which header your LEDs are on? Run `--set fixed --color ff0000 --header 1`, then
`--header 2` (see below), or try both channels in CoolerControl.

## Lighting modes

| Mode | Colours | Speed |
| --- | --- | --- |
| Fixed | 1 | – |
| Off (static black) | – | – |
| Breathing, Lightning, Marquee | 1 | ✓ |
| Flashing, Double Flashing, Meteor, Blink, Color Pulse, Color Wave, Visor, Stack | 1, or 0 for the board's rainbow colours | ✓ |
| Color Ring, Planetary, Double Meteor, Energy, Clock, Color Shift (color cycle), Rainbow Wave, Rainbow Flashing, Color Ring Double Flashing, Fire | board's rainbow colours | ✓ |

Speeds: `slow`, `medium`, `fast`. Run `msi_mysticlight --list-modes` for the internal names.
Effects run on the controller itself, so they keep going with no CPU cost.

## Command line tools

The binary doubles as a diagnostic tool. These commands need root, because the hidraw node
is root-only. Tool backups go to `./backups` unless you pass `--backup-dir`.

```bash
# Read only: print the controller state and save a backup (backups/mysticlight-dump-<time>.bin)
sudo ./target/release/msi_mysticlight --dump

# Backup, solid red on JRAINBOW1+2 for 10 s, rainbow wave for 10 s, then restore the backup
sudo ./target/release/msi_mysticlight --test 10

# Set a mode on one header (1, 2 or all)
sudo ./target/release/msi_mysticlight --set fixed --color 00aaff --header 1
sudo ./target/release/msi_mysticlight --set rainbow_wave --speed fast

# Write a saved backup back to the controller
sudo ./target/release/msi_mysticlight --restore backups/mysticlight-dump-<time>.bin

# List the lighting modes (no root needed)
./target/release/msi_mysticlight --list-modes
```

Add `-d` for debug logging. `cargo run --release --example probe -- <socket>` talks to a
running plugin over its Unix socket the way `coolercontrold` does (development helper).

## Known limitations

- **Settings are not saved to the controller's flash** (by design). After a cold boot the
  headers show the BIOS / MSI Center setting until CoolerControl applies yours again.
- **Zone-based effects only**: per-LED "direct" control (report `0x53`) is not implemented, so
  there are no software animations or per-LED colours.
- Only the two JRAINBOW (5 V ARGB) headers are exposed; the 12 V JRGB headers and the onboard
  LEDs are left untouched.
- One board is supported so far (see [Supported hardware](#supported-hardware)).
- The plugin reports no sensors or fans.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Device missing in CoolerControl | `lsusb \| grep 0db0:0076`; the logs (below) say why |
| "not a known 185-byte JRAINBOW1/2 board, refusing to drive it" | Your board is not on the allow-list; see [Supported hardware](#supported-hardware) |
| Nothing lights up | Check the header with `--set fixed --color ff0000 --header 1` / `--header 2`; make sure the LEDs are 5 V ARGB (3-pin) |
| Colours revert after reboot | Expected: nothing is saved to flash; CoolerControl re-applies at start |
| Something looks wrong | `sudo ./target/release/msi_mysticlight --restore backups/<file>.bin`, or Reset in CoolerControl, or a cold boot |

Logs:

```bash
journalctl -u cc-plugin-msi_mysticlight -f
journalctl -u coolercontrold -u cc-plugin-msi_mysticlight -f   # together with the daemon
```

The service saves the state it finds at start-up to
`/var/lib/coolercontrol/plugins/msi_mysticlight/backups/`.

## Protocol notes

- The controller is a USB HID device (`0db0:0076`); state is exchanged with **feature report
  `0x52`**, 185 bytes (report id + 184 data bytes), via `HIDIOCGFEATURE` / `HIDIOCSFEATURE`.
- The board code (first four characters of the USB serial) selects the board configuration.
  For `7E16`: 0 onboard LEDs, 2 JRGB, zones JRGB1, JRGB2, JRAINBOW1, JRAINBOW2.
- Layout (byte offsets, byte 0 = report id): JRGB1 @1, JPIPE1 @11, JPIPE2 @21,
  **JRAINBOW1 @31**, **JRAINBOW2 @42** (10-byte zone data + LED count), JCORSAIR @53,
  onboard LEDs @74, JRGB2 @174, save flag @184.
- Zone data: effect, colour (RGB), speed / brightness / sync flags, second colour, colour
  flags, padding, LED count.
- A write follows what OpenRGB does for effect modes: read report `0x52`, send it back, then
  send the modified report. The save-to-flash byte is always `0`.

See [`src/protocol.rs`](src/protocol.rs) (with unit tests for the layout and the write logic)
and [`src/hid.rs`](src/hid.rs). Run the tests with `cargo test`.

## Disclaimer

This is an **unofficial** project. It is not affiliated with, endorsed by, or supported by
Micro-Star International (MSI), OpenRGB, or the CoolerControl project. "Mystic Light" is a
trademark of MSI. The plugin writes to your motherboard's RGB controller. It never writes to
flash and refuses unknown boards, but you use it **at your own risk**; the authors take no
responsibility for any damage to your hardware or data.

## Credits

- **[OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB)** (GPL-2.0-or-later): the Mystic
  Light protocol (packet layout, zone and flag handling, mode list and board configuration) is
  adapted from `Controllers/MSIMotherboardController/` (`MSIMotherboardCommon.h`,
  `MSIMotherboard185Controller.cpp`, `RGBController_MSIMotherboard185.cpp`) by T-bond,
  Adam Honse and the OpenRGB contributors, building on the direct-mode research of the
  mystic-why project (Aleksandr Garashchenko). Used under the "or any later version" clause
  of GPL-2.0-or-later and distributed here under GPL-3.0-or-later. Reference revision: OpenRGB
  commit `d09a493f8b44`.
- **CoolerControl** and its
  [Device Service plugin template](https://gitlab.com/coolercontrol/cc-plugins)
  (GPL-3.0-or-later); the files in `proto/` come from CoolerControl.

## License

[GPL-3.0-or-later](LICENSE).
