<p align="center">
  <img src="docs/banner.png" alt="Ritardometro Banner" width="100%">
</p>

# Ritardometro

The smart speaker pacing timer that tells you **whether you are ahead or behind schedule**, featuring an animated speaker caricature cheering you on (or panicking) in front of your audience.

![Avatar states](docs/stati.png)

Ritardometro stays always on top of your presentation, reads your current slide number from the screen in real-time, and compares it against your target pacing schedule:

- **Green aura & beacon**: you are ahead of time, feel free to slow down or elaborate
- **Red aura & beacon**: you are behind schedule, speed up
- **Expressive cartoon caricature**: smiles and nods when you are on track, frowns and worries when you fall behind, widens eyes and breaks into comic sweat drops when panic kicks in
- **Milestone speech balloons**: pop up at halfway (`HALF WAY!`), 5 minutes remaining (`5 MIN LEFT`), Q&A session (`Q&A?`), talk conclusion (`GREAT!`), and time expired (`STOP!`)

![Milestone balloons](docs/bandierine.png)

## How It Works

The core philosophy is **one slide per minute** (or your custom slide pacing ratio): for a 30-minute talk with 30 slides, at minute 12 you should ideally be around slide 13. Transition slides allow you to catch up, and Ritardometro gives you immediate live feedback.

1. A background thread captures the screen slide counter area (by default, the bottom-right corner where Marp and PDF presentation viewers display pagination). If nothing is readable there, it automatically scans the other corners and locks onto the one showing the counter.
2. The native OCR engine extracts the current slide and total (e.g. `14 / 35`).
3. Ritardometro calculates your time difference in seconds, updates the animated avatar, and adjusts the progress gauge.

A slide number is only shown, and only used for pacing, while it is still being confirmed. If the OCR stops finding the counter — a full-bleed slide, a section divider, OCR switched off — the last number is kept for about one and a half slides and then dropped: without it the slide you are on cannot be known, and a frozen number would show a delay that grows on its own without anyone having said anything wrong. Ritardometro then goes back to being an honest stopwatch (a slide number you set by hand with the ← → arrow keys is yours, and never expires).

When the slides carry no counter at all (so there is nothing to read), Ritardometro watches the screen instead: if it does not change for longer than `stuck_factor` times the average time per slide (`duration / total_slides`), a pulsing watch appears next to the avatar to tell you that you are dwelling. This runs on Windows and macOS; on macOS the system asks for Screen Recording permission the first time, and `stuck_factor = 0` turns the whole thing off.

## Installation

Download the archive for your operating system from the [Releases page](https://github.com/matteobaccan/Ritardometro/releases/latest) and extract it anywhere:

| Platform | Archive |
|---|---|
| Windows 10/11 (x64) | `ritardometro-windows-x64-vX.Y.Z.zip` |
| Linux (x64) | `ritardometro-linux-x64-vX.Y.Z.tar.gz` |
| macOS Apple Silicon (M1/M2/M3…) | `ritardometro-macos-arm64-vX.Y.Z.tar.gz` |
| macOS Intel | `ritardometro-macos-x64-vX.Y.Z.tar.gz` |

> Automatic slide OCR detection runs natively on Windows via `Windows.Media.Ocr`. On macOS, Ritardometro watches the screen for slide changes instead (see `stuck_factor` below). On Linux it works as a full-featured pacing timer (with manual slide stepping via ← → arrow keys).
> On macOS the binary is unsigned: on first launch, use right-click → Open, or run `xattr -d com.apple.quarantine ./ritardometro`.
> On macOS the overlay joins every Space, so it stays visible over full-screen presentations (Preview, Keynote, PowerPoint), which run in a Space of their own.

The installed version and repository link are always visible at the bottom of the Settings window (right-click the avatar).

Or build it from source (requires [Rust](https://rustup.rs)):

```sh
cargo build --release
# binary is created in target/release/ritardometro(.exe)
```

## Publishing a New Version

1. Bump `version` in `Cargo.toml` (e.g. `1.1.0`) and commit.
2. Create and push the corresponding tag:
   ```sh
   git tag v1.1.0
   git push origin v1.1.0
   ```
3. The [`release.yml`](.github/workflows/release.yml) GitHub Action automatically validates the version, cross-compiles release binaries for Windows, Linux, and macOS (Apple Silicon + Intel), generates `SHA256SUMS.txt`, and publishes the release.

## Usage

```sh
ritardometro.exe                          # uses ritardometro.toml located next to executable
ritardometro.exe devsecopsday.toml        # custom config file for a specific talk
ritardometro.exe --minutes 25 --slides 28 # override duration and slide count on the fly
ritardometro.exe --demo                   # demo mode: simulates a 30-minute talk in ~1 minute
```

| Option | Description |
|---|---|
| `file.toml` | config file to load (default: `ritardometro.toml` next to the executable) |
| `-m N`, `--minutes N` | talk duration in minutes (accepts decimals, e.g. `12.5` or `12,5`) |
| `-s N`, `--slides N` | total slide count (`0` = auto-detect total from screen e.g. "12 / 40") |
| `--demo` | demo mode: accelerated 30x talk simulation without OCR (also plays the pulsing watch, about 33 s in) |
| `-h`, `--help` | display command line help |

On first launch, a documented `ritardometro.toml` file is automatically created.

Before stepping on stage:

1. Open your presentation slides in full screen (duplicated display).
2. Launch Ritardometro: it appears in your configured corner (default: top-right); drag and resize it anywhere, including across multiple monitors. Where you leave it is where it comes back next time — unless that spot is no longer on any screen (an unplugged monitor), in which case it returns to the configured corner.
3. It starts automatically as soon as you transition from slide 1 to slide 2, or upon first click.

### Keyboard Shortcuts

Keys work when the overlay is focused (click on it); during your talk, your focus stays on the presentation viewer so slide clickers and arrow keys advance slides normally.

| Key | Action |
|---|---|
| `Space` | Start / Pause timer (in pause, OCR stops capturing the screen) |
| `R` | Reset timer and milestone flags |
| `+` (or `=`) | Elapsed time +1 minute |
| `-` | Elapsed time −1 minute |
| `→` or `PageDown` | Next slide (manual correction or non-OCR mode) |
| `←` or `PageUp` | Previous slide |
| `Ctrl` + `+` | Increase overlay size |
| `Ctrl` + `-` | Decrease overlay size |
| `Ctrl` + `0` | Reset size to 100% |
| `Esc` or `Q` | Close Settings window if open, otherwise quit the application |

### Mouse Controls

| Gesture | Action |
|---|---|
| Left click on overlay | Start / Pause |
| Left click and drag | Move overlay (smooth multi-monitor support) |
| Drag edge or corner | Corner-anchored resize (the opposite corner stays fixed) |
| Mouse wheel | Zoom / resize overlay smoothly |
| Right click | Open / close Settings window |

### Settings Window (Right Click)

Every setting lives here: there is nothing you have to edit by hand. Changes apply straight away and are written back to `ritardometro.toml` a moment later, comments and all, so they are still there at the next launch.

The controls you reach for *during* a talk stay visible above the tabs:

| Always visible | Description |
|---|---|
| Pause / Start, Reset | Same as `Space` and `R` |
| Time −1 / +1 | Adjust the elapsed talk time |
| Slide −1 / +1 | Step the slide number by hand |

Everything else is grouped in four tabs:

| Tab | What is in it |
|---|---|
| **Talk** | Talk duration, question minutes, total slides, auto start, on-track tolerance, panic threshold, and the dwell watch factor |
| **Overlay** | Size, background opacity, avatar & text opacity, starting corner, margin, and a button to move the window back to that corner |
| **OCR** | OCR on/off, monitor, automatic counter detection, the capture rectangle in percentages, a button to flash it on screen, polling rate, magnification and the debug capture |
| **Milestones** | The speech balloons: add or remove them, set each one at a percentage of the talk or at minutes left, and type its text |

The footer carries the version, the link to GitHub and the Close / Quit buttons.

Two settings deserve a note. `--minutes` and `--slides` given on the command line apply to that run only and are deliberately *not* written to the file. And a configuration file that cannot be parsed is never overwritten: Ritardometro falls back to the defaults and leaves your file alone.

## Configuration

All options available in `ritardometro.toml`:

```toml
duration_minutes = 30     # Talk duration in minutes (excluding Q&A)
questions_minutes = 10    # Q&A countdown duration in minutes (0 = none)
total_slides = 30         # Total slides (0 = auto-detect total from screen e.g. "12 / 40")
auto_start = true         # Auto-start timer when advancing from slide 1 to 2
tolerance_seconds = 60    # Seconds of delta considered "on track"
panic_seconds = 180       # Seconds behind schedule before avatar panics
scale = 1.0               # Initial overlay scale factor
background_opacity = 0.70 # HUD background box opacity (0.0 - 1.0)
opacity = 1.0             # Avatar and text opacity (0.15 - 1.0)
stuck_factor = 1.5        # No slide counter: show the pulsing watch after this many
                          # times the average time per slide (0 = never watch the screen)
position = "top-right"    # Initial corner: "top-left", "top-right", "bottom-left", "bottom-right"
margin = 24               # Margin from screen edge in points

[ocr]
enabled = true            # false = timer-only mode (no OCR)
monitor = 0               # 0 = auto (follows overlay monitor), 1, 2, 3... = fixed monitor
auto_zone = true          # look for the counter in the other corners if the box below reads nothing
# Slide counter bounding box in screen fractions (default: bottom-right corner)
x = 0.85
y = 0.88
width = 0.15
height = 0.12
interval_ms = 250         # OCR capture interval in milliseconds
magnification = 3         # Pre-OCR scaling factor
save_debug = false        # true = save capture to ritardometro-debug.png

[[milestone]]
percentage = 50
text = "HALF WAY!"

[[milestone]]
minutes_left = 5
text = "5 MIN LEFT"
```

You can define as many `[[milestone]]` sections as needed, using either `percentage` of talk duration or `minutes_left`.

Anything you change in the Settings window — size, transparency, OCR on/off, monitor, polling rate — is written back into this file shortly after, keeping every comment in place, so it is there again at the next launch. The window position is saved the same way, in a `[window]` section the program manages on its own. Editing the file by hand while Ritardometro is running still works, but the keys above are rewritten from the app: close it first. If the file cannot be parsed, Ritardometro falls back to the defaults and stops writing to it, so a broken file is never overwritten.

### Troubleshooting OCR

- OCR captures the **monitor display region** (independent of window focus): with `monitor = 0` it captures the monitor where Ritardometro is located. Use **Highlight OCR Zone on Screen** in Settings to inspect which screen and bounding box are being captured.
- If your slides number pages somewhere other than the bottom-right corner, you normally do not need to configure anything: after a few seconds without a reading, Ritardometro scans the other corners and stays on the one where it finds the counter (the status line then shows e.g. `OCR ok: slide 12/40 @ bottom-left`). Set `auto_zone = false` to always capture the configured box only.
- Enable **Save ritardometro-debug.png** in Settings (or `save_debug = true`) and inspect `ritardometro-debug.png`: it should tightly frame your slide counter numbers. Adjust `x`, `y`, `width`, `height` if needed.
- Do not place the Ritardometro overlay directly on top of the slide counter: it would capture its own slide number. With `auto_zone = true` it detects the overlap and moves the capture to another corner by itself.
- Windows OCR uses installed system language packs. If you see an OCR unavailable message, install a language pack with optical character recognition under *Settings > Time & Language > Language*.
- In Marp presentations, a formatted counter like `12 / 40` works reliably:

  ```css
  section::after { content: attr(data-marpit-pagination) ' / ' attr(data-marpit-pagination-total); }
  ```

## Architecture

| File | Role |
|---|---|
| `src/main.rs` | CLI arguments, window setup (including the caricature as window icon) and entry point |
| `src/app.rs` | Main overlay application: talk state machine, rendering, resizing, multi-monitor dragging, settings window |
| `src/avatar.rs` | Animated 2D deformable mesh caricature (facial expressions, breathing, sweat drops, comic speech balloons, pulsing watch) |
| `src/pacing.rs` | Pure pacing logic: stopwatch, delta calculation, pacing judgments, milestones, how long a slide reading stays trustworthy (with unit tests) |
| `src/ocr/` | Screen capture (`xcap`), Windows OCR integration, regex parsing, automatic search for the slide counter zone |
| `src/schermo.rs` | Screen-change detection for presentations without a counter: greyscale fingerprint, monitor geometry (Windows and macOS) |
| `src/config.rs` | `ritardometro.toml` loading, validation and defaults |

UI rendered via [egui/eframe](https://github.com/emilk/egui) with OpenGL (Glow) backend for rock-solid transparent window composition across Windows, Linux and macOS.

```sh
cargo test   # run pacing logic, OCR parser, screen-change detection and configuration tests
```

## License

MIT
