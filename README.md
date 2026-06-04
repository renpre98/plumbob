# Plumbob Link

Drive the **SteelSeries Sims 4 Collector's Edition USB Plumbob** from Paralives.
The dominant emotion of the actively selected Parafolk is mirrored on the
real-world Plumbob in real time, with smooth color fades.

> [insert short demo gif / video here]

---

## What's in the box

| Component | Role |
|---|---|
| `PlumbobLink.dll` | BepInEx 5 plugin for Paralives — reads the active Para's emotion via Harmony patch and POSTs it to the daemon |
| `plumbob-daemon` | Rust daemon that owns the USB device and translates HTTP requests into RGB writes |
| `plumbob` | Standalone CLI (`plumbob <R> <G> <B>`, `plumbob demo`, `plumbob off`) — useful for tests or scripting without the daemon |
| `plumbob-daemon.service` | systemd user unit that auto-starts the daemon when the Plumbob is plugged in |
| `70-plumbob.rules` | udev rule granting the logged-in user write access to the device |
| `install.sh` | Idempotent installer that wires the above into a working setup |

---

## Requirements

- **The hardware**: a SteelSeries Sims 4 Collector's Edition USB Plumbob (product 60038, USB ID `1038:1500`). This device was only sold with the Sims 4 Collector's Edition — yes, it's rare.
- **Linux**. The daemon talks to `/dev/hidraw*` directly; there is no Windows build yet. Tested on Kubuntu 24.04.
- **Paralives** (Steam, Early Access) on the same machine, running through Proton.
- **BepInEx 5.4.x for Paralives** ([6xvl/paralives-plugins-index](https://github.com/6xvl/paralives-plugins-index) or upstream BepInEx).
- Rust 1.80+ and .NET SDK 6.0+ if you want to build from source.

---

## Install

### 1. Install the daemon (Linux side)

Unpack the release archive, then:

```bash
cd plumbob/
./install.sh
```

This installs:

- `plumbob-daemon` → `~/.local/bin/`
- `plumbob-daemon.service` → `~/.config/systemd/user/` (auto-started on login when the Plumbob is plugged in)
- `70-plumbob.rules` → `/etc/udev/rules.d/` (requires `sudo`; grants the device to your user via the `uaccess` tag)

Verify with:

```bash
systemctl --user status plumbob-daemon
curl -X POST http://127.0.0.1:27301/emotion \
    -H 'Content-Type: application/json' \
    -d '{"emotion":"Happy"}'
```

The Plumbob should fade to green.

### 2. Install BepInEx into Paralives

If you don't already have BepInEx in your Paralives install:

1. Download [BepInEx 5.4.23.2 (win_x64)](https://github.com/BepInEx/BepInEx/releases/tag/v5.4.23.2)
2. Extract the zip into your Paralives install folder (the one that contains `Paralives.exe`).
3. In Steam, right-click **Paralives → Properties → Launch Options** and add:
   ```
   WINEDLLOVERRIDES="winhttp=n,b" %command%
   ```
4. Launch Paralives once and close it again — BepInEx generates its config files.

### 3. Drop in the plugin

Copy `PlumbobLink.dll` into:

```
<Paralives>/BepInEx/plugins/
```

Launch Paralives. Load any household. The Plumbob now follows the active Parafolk's mood.

---

## Configuration

The plugin generates a config file at `BepInEx/config/de.redclu.plumboblink.cfg`. The defaults are sensible; tweak if needed:

| Key | Default | What it does |
|---|---|---|
| `Daemon.Url` | `http://127.0.0.1:27301/emotion` | Endpoint to POST to. Override to redirect or proxy. |
| `Daemon.FadeMs` | `800` | Crossfade duration sent to the daemon. |
| `Throttling.MinIntervalSeconds` | `0.5` | Floor on POST frequency per character to avoid spam. |

---

## How it works

```
┌──────────────────────────────────────┐
│ Paralives (Proton) + BepInEx 5       │
│   PlumbobLink.dll                    │
│     · HarmonyX Postfix on            │
│       UIEmotions2.Update             │
│     · reads _previousCharacter +     │
│       _emotionValues via reflection  │
│     · HTTP POST                      │
└────────────────┬─────────────────────┘
                 │ 127.0.0.1:27301 (TCP, Wine→Linux loopback)
                 ▼
┌──────────────────────────────────────┐
│ plumbob-daemon (Rust, tokio + axum)  │
│   · /emotion, /game_event, /rgb      │
│   · color mapping + fade engine      │
│   · writes 32-byte OUTPUT report     │
│     directly to /dev/hidraw*         │
└────────────────┬─────────────────────┘
                 │
                 ▼  SteelSeries Plumbob (USB 1038:1500)
```

Port `27301` is the SteelSeries GameSense default port, so the same daemon can also accept Sims 4 GameSense events without modification.

---

## Troubleshooting

**Plumbob doesn't react and `systemctl --user status plumbob-daemon` shows _condition not met_.**
The device isn't visible to udev. Replug, then `systemctl --user restart plumbob-daemon`.

**`curl` works but Paralives doesn't trigger color changes.**
Check `<Paralives>/BepInEx/LogOutput.log` — you should see `Plumbob Link 1.0.0 loaded` at startup. If not, BepInEx didn't load: re-verify the Steam launch option `WINEDLLOVERRIDES="winhttp=n,b" %command%`.

**Log says `plumbob-daemon not reachable`.**
Daemon isn't running. Start with `systemctl --user start plumbob-daemon` or run `~/.local/bin/plumbob-daemon` in a terminal for live output.

**Two daemons fighting for the device.**
The daemon claims the hidraw node exclusively. If you ran one manually and then `install.sh` started a systemd instance, kill the manual one (`Ctrl-C` or `pkill plumbob-daemon`).

**A new Paralives patch broke the mod.**
Open an issue with the offending version. The mod relies on reflected field/method names in `UIEmotions2` and `Settings`; a Paralives update may rename them.

---

## Building from source

```bash
# daemon + CLI
cargo build --release
# the plugin (needs dotnet SDK + a local copy of BepInEx + Paralives DLLs)
cd mods/paralives && dotnet build -c Release
```

Build dependencies are intentionally minimal — the daemon uses only `anyhow`, `tokio`, `axum`, `serde`, and `tracing` (no `libudev`, no `hidapi`).

---

## Limitations

- **Linux only** today. A Windows daemon is on the roadmap; contributions welcome.
- **Paralives-only** game side at present. The daemon already speaks GameSense-style `/game_event`, so a Sims 4 build is mostly configuration; a Sims 3 hook would need a script mod.
- Mod reads private fields of `UIEmotions2` via reflection — robust against minor refactors, but a sufficiently large Paralives update can break it.

---

## Credits

- Hardware reverse-engineering started from [TS4PlumbobController](https://sourceforge.net/projects/ts4plumbobcontroller/) (Mac, 2014) — that's where the USB VID/PID and 32-byte report layout came from.
- BepInEx / HarmonyX teams for the modding stack.
- Paralives team for Paralives.

## License

MIT — see [LICENSE](LICENSE).
