# p1-logger

A Rust daemon that reads a Dutch/Benelux smart meter's P1 port (DSMR5) over
UART on a Raspberry Pi, validates and parses the telegrams, and logs them to
SQLite — one file per UTC calendar day. Built to run unattended for months:
crash-safe writes (WAL), a read loop that never blocks on disk I/O, and
graceful handling of the line noise a real P1 cable might pick up.

The eventual goal is per-minute/per-day aggregates and simple switching logic
(water heater, diverter valve, washing machine) based on momentary surplus/deficit from solar.

## How it's put together

- `src/parser.rs` — parses one DSMR5 telegram: CRC16/ARC validation, OBIS-code
  mapping, timestamp conversion (handles the S/W summer/winter flag), values
  converted to integer milli-units (no floats, ever).
- `src/telegram.rs` — frames raw serial bytes into complete telegrams. A
  small state machine (`AwaitingFirstHeader` / `Accumulating` / `AfterFooter`)
  distinguishes ordinary line noise from things that should never happen
  (data sitting in the gap between a footer and the next header).
- `src/db.rs` — one SQLite file per day, WAL mode, batched inserts.
- `src/io_source.rs` — opens a path as a live serial port if it's a character
  device, or reads it as a plain file otherwise. Lets the exact same binary
  run against real hardware or a captured telegram file for testing.
- `src/args.rs` — CLI argument parsing.
- `src/main.rs` — setup and glue only: parses args, wires up logging, spawns
  a writer thread connected to the reader via an unbounded channel (so a slow
  SD card can never stall the UART read), and dispatches `TelegramEvent`s.

## Building and running

```bash
./run <file>                  # run against a file or a serial device path
./run build-dev                # cargo build (debug, native)
./run build-release            # cargo build --release (native)
./run build-pi                 # cross-compile for the Pi (aarch64-unknown-linux-gnu, via `cross`)
./run deploy                   # build-pi, then scp the binary + systemd unit to the Pi
```

`./run deploy` reads `PI_HOST`/`PI_USER`/`DEPLOY_PATH` from a local `.env`
file (gitignored) — `PI_USER` (e.g. `john`) is substituted into
[systemd/p1-logger.service](systemd/p1-logger.service)'s `__PI_USER__`
placeholder at deploy time (used for both `User=` and the `/home/__PI_USER__`
paths), so the committed unit file carries no personal username or paths.

### CLI flags

```
p1-logger <serial-device-or-file> --data-dir <dir> [--bad-telegram-dir <dir>] [-d] [-l] [-q]
```

- `--data-dir` — required; where the day SQLite files go. No default, on
  purpose: the one caller that matters (the systemd unit) always passes an
  explicit absolute path, so a silent relative-to-CWD fallback would only
  ever paper over a forgotten flag.
- `--bad-telegram-dir` — if set, every rejected/anomalous telegram gets
  dumped there as a `<EventKind>-<timestamp>-<n>.txt` file, for offline
  inspection of what the actual noise looked like.
- `-d` — also print every parsed record to stderr (testing).
- `-l` — send warnings/errors also to syslog (`/dev/log`).
- `-q` — suppress stderr entirely. Normal unattended operation is `-l -q`.

## Hardware: reading the P1 port

### Wiring

The P1 connector is RJ12 (6P6C), but only pins 2–5 are used — an RJ11 (6P4C)
physically fits the same jack and covers exactly this range.

| P1 pin | Signal                          | Goes to                              |
|-------:|----------------------------------|---------------------------------------|
| 2      | REQUEST                          | Pi's own 5V (header pin 2) — *not* from the meter |
| 3      | Data-GND                         | Pi GND                                |
| 5      | Data (open-collector, inverted)  | Inverter circuit below, into RXD      |

Pin 1 (meter's own 5V) and pin 6 are unused — the Pi is powered separately.

### Inverter circuit (BS170)

The Pi's UART has no hardware invert bit (unlike an ESP32/STM32), and P1's
data line is open-collector and inverted, so a small MOSFET inverter sits in
between:

```
        +5V                                    +3.3V
         │                                        │
      ┌──┴──┐                                  ┌──┴──┐
      │ 10k │                                  │ 10k │
      └──┬──┘                                  └──┬──┘
         │                                        │
         ├── P1 pin 5 "DATA"                      ├── GPIO15 / RXD (header pin 10)
         │   (open-collector, inverted)           │
        Gate                                    Drain
         └──────────────[ BS170 ]────────────────┘
                             │
                          Source
                             │
                            GND ── P1 pin 3 "Data-GND"  +  Pi GND
```

**Why the gate pull-up is 5V but the drain pull-up stays 3.3V:** these are
two independent decisions, not a symmetric choice.

- The BS170's gate threshold voltage (Vgs(th)) varies per part, typically
  0.8–3V. With the gate pulled to only 3.3V, the "on" drive sits close to
  that threshold — not much margin, so a brief noise dip near the switching
  point could cause a false transition. Pulling the gate to 5V instead gives
  a lot more overdrive above the threshold, so the switch snaps decisively
  instead of lingering near the edge. The BS170's gate is rated for roughly
  ±20V, so 5V here is well within spec.
- The drain pull-up **must** stay on 3.3V, because the drain connects
  directly to GPIO15. Raspberry Pi GPIO pins are **not 5V-tolerant** — 5V on
  that side risks damaging the Pi.

So: gate side → 5V (more noise margin, safe for the MOSFET), drain side →
3.3V always (safe for the Pi). Don't swap these.

### Freeing up the UART on the Pi

By default, Bluetooth claims the full PL011 UART, leaving GPIO14/15 with the
inferior mini-UART (clock tied to CPU frequency, less stable). Disable it:

```bash
# /boot/firmware/config.txt, under [all]:
dtoverlay=disable-bt
```

Then disable the serial console/login shell (but keep the serial *hardware*
enabled) via `raspi-config`:

```bash
sudo raspi-config
# Interface Options -> Serial Port
#   "login shell over serial?"      -> No
#   "serial port hardware enabled?" -> Yes
sudo reboot
```

Verify after reboot:

```bash
dmesg | grep -i bluetooth        # should print nothing
ls -l /dev/serial0                # should symlink to /dev/ttyAMA0
```

Manual UART config for ad-hoc testing (the daemon itself already configures
this when it opens the port):

```bash
stty -F /dev/serial0 115200 cs8 -cstopb -parenb raw
```
Test:
```bash
cat /dev/serial0
```


## Deploying as a service

```bash
./run deploy
```

Then, one-time on the Pi:

```bash
sudo mv ~/p1-logger.service /etc/systemd/system/p1-logger.service
sudo systemctl daemon-reload
sudo systemctl enable --now p1-logger
```

After that, `./run deploy` + `sudo systemctl restart p1-logger` is all a
redeploy needs. See [systemd/p1-logger.service](systemd/p1-logger.service)
for the unit itself (`Restart=always`, runs as an unprivileged user).
