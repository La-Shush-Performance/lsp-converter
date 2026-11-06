# LSP Converter

LSP Converter turns Assetto Corsa car mods into Assetto Corsa EVO mods, on
your own PC, from your own installs of both games. It's made by La Shush
Performance and it's free.

It does one job. It doesn't host servers, connect to anything online, change
the game's own install or ship any Kunos content. Everything it builds from
comes from the copies of AC and AC EVO you already own on Steam.

Runs on Windows 10/11 (Edge WebView2 is included with Windows 11).

## Using it

1. Unzip it anywhere and open **LSP Converter.exe**. There's no installer.
   It isn't code-signed yet, so Windows may say it "protected your PC":
   click **More info**, then **Run anyway**.
2. **Convert** lists the cars in your AC install. Pick one, convert it, and
   the result is checked and installed into your EVO mods folder
   (`Saved Games\ACE\mods`), where the game finds it.
3. **Settings** shows the two folders it uses (your EVO install and your
   EVO mods folder). They're found automatically; only fill one in if it
   was found wrong.
4. **Diagnostics → Logs** shows everything it did, with the full error for
   anything that went wrong.

Its settings, logs and work files live in `%LOCALAPPDATA%\LSP Converter`.

### What it won't convert

- Cars protected with CSP encryption: it tells you so and stops.
- Kunos's own AC cars: they belong to Kunos.
- Tracks: putting a converted track in the game means changing the game's
  own `content.kspkg`, so LSP Converter sticks to cars.

### Found a bug, or want a feature?

Tell us on our Discord, https://discord.gg/zjq45ZCJCt, or email
contact@lashushperformance.com, with the car and what happened.
**Diagnostics → Logs** has the log we may ask for.

## Build from source

You need the Rust MSVC toolchain.

```bash
cd native
cargo build --release   # -> lsp-converter.exe, with the converter and UI inside
```

The version lives in `native/Cargo.toml` and `native/tauri.conf.json`; keep
them the same. The web UI is in `web/`.

The converter itself (`converter/`, `aceconv`) is its own Rust crate.
`native/build.rs` builds it and embeds it in the app. It can also be run on
its own (`aceconv car <ac1 car> <out>`, `aceconv check-car <car>`); its
formats are documented in [`converter/docs/FORMATS.md`](converter/docs/FORMATS.md).

## Where it comes from

LSP Converter is a modified version of
[ACECM](https://github.com/WhoaThatCombo/AC-evo-content-manager-) (Assetto
Corsa EVO Content Manager) by WhoaThatCombo and the ACECM contributors,
forked at ACECM commit `18b6d272bddb`. ACECM's converter is where it started;
the rest of ACECM (its servers, lobby, sharing, game patching and updater)
has been removed.

Since then La Shush Performance has reworked how it converts: written
conversion rules that apply to every car, a long list of handling and sound
behaviours corrected, and new behaviours of our own, each one checked by
comparing MoTeC telemetry and by driving the cars on track. What changed, and
when, is in [FORK_NOTES.md](FORK_NOTES.md).

Copyright © 2026 La Shush Performance. Based on ACECM, copyright ©
WhoaThatCombo and the ACECM contributors.

It's free software under the GNU General Public License, version 3 or
later ([LICENSE.txt](LICENSE.txt)); the copyright notices, ours and
ACECM's, are in [NOTICE.txt](NOTICE.txt). Third-party code and its licences
are listed in [CREDITS.md](CREDITS.md).

The La Shush Performance name, the LSP badge, crest and monogram are La
Shush Performance's trademarks, not covered by the GPL.

LSP Converter isn't affiliated with Kunos Simulazioni or 505 Games.
Assetto Corsa and Assetto Corsa EVO are their trademarks.
