# Working on Ritardometro

An always-on-top overlay that tells a speaker whether they are ahead or behind
schedule, with an animated caricature. Rust, `eframe`/`egui` with the Glow
backend, no runtime dependencies beyond the OS.

This file is the project's memory: the decisions behind the code, so the next
person — or agent — does not undo them by accident.

## Checks before any push

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

CI runs both on Windows, Linux and macOS, and clippy with `-D warnings`, so a
warning fails the build. Much of the code is platform gated: check the targets
you are not on, otherwise you find out from CI.

```sh
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin
cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings
```

`cargo run -- --demo` plays an accelerated talk and is the only way to see the
avatar's states without giving a real one; the pulsing watch shows up about 33
seconds in with the default configuration.

## Language

Identifiers and comments are in **Italian**; everything the user or another
developer reads from outside — interface strings, configuration keys, README,
commit messages, pull request text — is in **English**. Comments explain *why*,
not what the line does.

## Everything is configurable from the GUI

The rule the project is held to: **no setting may require editing the file by
hand**. A new parameter ships in the same change as its control in the Settings
window, its persistence, and its live application. The window has no "open the
configuration file" button on purpose.

## The configuration file

`ritardometro.toml` is written back by the program, and `src/config.rs` does it
**key by key, never by re-serialising the struct**: the comments in that file
are the documentation of every parameter and a full rewrite would erase them.

- `[[milestone]]` is a list, so it cannot be patched like a key: the blocks are
  dropped and rewritten at the end. Comments written *inside* a block are the
  one thing the program cannot preserve.
- Removing every milestone writes an explicit `milestone = []`. An absent key
  means "use the defaults", and the two built-in ones would come back.
- A file that fails to parse is **never** overwritten: the program runs on the
  defaults and leaves it alone (`Caricata::salvabile`).
- `--minutes` and `--slides` apply to that run only and are not written back
  (`Caricata::da_cli`). A flag passed for one talk must not become the default.
- `[window]` is machine state the program manages by itself.

## Modes

`mode` is `"ocr"`, `"screen"` or `"timer"`:

- **OCR** reads the slide counter. The pulsing watch stays on as a **safety
  net** for when the counter disappears — this is deliberate, not an oversight.
- **Screen** reads nothing and only watches whether the screen changes.
- **Timer** captures nothing at all. It is the only mode macOS never asks
  Screen Recording permission for, which is most of its value.

`Config::mode` defaults to `None`, not to `Some(Ocr)`: with `#[serde(default)]`
any other default would be used for files written before the key existed, and
the inference from the old switches would never run. `ocr.enabled` is kept in
step with the mode because it is still what the capture thread reads each
cycle, and `stuck_factor` is only a threshold — zero turns the watch off
without leaving the mode.

## A slide number is only as good as its last reading

An OCR reading is believed for `pacing::scadenza_lettura` — one and a half
average slides, between 20 seconds and 3 minutes. After that the slide counts
as unknown and **everything derived from it disappears**: numbers, the tick on
the bar, ahead/behind. A frozen number would show a delay that grows on its own
while the speaker has done nothing wrong, and a wrong answer is worse than
none. A number set with the arrow keys is the speaker's assertion and never
expires.

## Platforms

| | Windows | macOS | Linux |
|---|---|---|---|
| OCR (`Windows.Media.Ocr`) | yes | no | no |
| Screen-change detection (`xcap`) | yes | yes | no |

`xcap` demands `libpipewire-0.3` at build time on Linux, which is why it is not
compiled there; including it means that dependency plus a line in the CI
workflow. On macOS the overlay only stays above full-screen presentations
because `src/app.rs` sets the `NSWindow` `collectionBehavior` through AppKit:
neither eframe nor winit exposes it.

## Drawing

The caricature is one texture deformed live: a 40 × 52 mesh whose vertices are
displaced by `deforma_uv`, a sum of gaussian weights around facial landmarks.
Anything drawn on top of the face (sweat, balloons, tension marks) asks
`deforma_uv` where that point is right now, so it follows the deformation.

Everything is drawn ink outline first, fill second. HUD text and the progress
bar carry a dark contour at all times: with `background_opacity = 0` the
overlay sits straight on the slide, and white on white is invisible.

## Releases

1. Bump `version` in `Cargo.toml` and merge it.
2. Tag `vX.Y.Z` on `main` and push it. The workflow refuses to build if the tag
   and the version disagree.
3. It builds four archives named **product-system-version**
   (`ritardometro-windows-x64-v1.2.0.zip`), with the zip holding its files at
   the root — Windows already creates a folder when extracting — and the
   tar.gz keeping its top-level directory, because tar wraps nothing and would
   otherwise spill files into the current directory. Release notes are
   generated from the merged pull requests; there is no changelog file.

In Claude Code cloud sessions the GitHub credentials may refuse writes to
`refs/tags` and branch deletions. Do not fight it: ask the maintainer to push
the tag.

## Deliberately not done

Do not "fix" these without asking — they were decided, not forgotten:

- counting screen changes as slide advances (it would make Screen mode report
  pacing, but builds and animations inside a slide make the count drift, and a
  wrong delay is worse than none);
- the detector on Linux, for the build dependency above;
- `docs/stati.png` does not show the pulsing watch.
