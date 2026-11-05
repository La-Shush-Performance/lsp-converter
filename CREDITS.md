# Credits

LSP Converter, by La Shush Performance, is a modified version of ACECM
(Assetto Corsa EVO Content Manager) by the ACECM contributors. Both are free
software under the GNU General Public License, version 3 or later
(LICENSE.txt). Neither is affiliated with Kunos Simulazioni or 505 Games.
Assetto Corsa and Assetto Corsa EVO are their trademarks. No game content is
included: everything is read from the user's own installation.

## Bundled programs

- **aceconv** (`converter/`): the Assetto Corsa -> Assetto Corsa EVO car and
  track converter, GPL-3.0-or-later. Its own credits are in
  `converter/CREDITS.md`.

## Map data

The Drift game's circuit The OG follows the centre line of the Tsukuba
Circuit 2000 course in OpenStreetMap: © OpenStreetMap contributors,
available under the Open Database License (openstreetmap.org/copyright).

## Libraries (Rust crates)

The app and the converter are built from crates.io crates whose licences are
all compatible with the GPL-3.0: by count, mostly MIT and/or Apache-2.0, then
BSD-3-Clause, ISC, Zlib, Unlicense, Unicode-3.0, BSL-1.0, CC0-1.0, 0BSD and
MPL-2.0 (cssparser, selectors: file-level copyleft, compatible with GPL-3.0
through its secondary-licence clause). `THIRD-PARTY-NOTICES.txt` lists every
crate inside the app and the converter, with the licence and notice files
each one publishes. It's made by `tools/third-party-notices.ps1`, ships with
the app, and shows on its About page.

Notable ones:

| crate | licence | use |
|---|---|---|
| tauri, wry, tao | MIT / Apache-2.0 | the window (WebView2) |
| axum, hyper, tokio, reqwest | MIT | the local server (this PC only) |
| image, png, texture2ddecoder | MIT / Apache-2.0 | pictures |
| zip, tar, flate2 | MIT / Apache-2.0 | archives |
| intel_tex_2 (ISPC Texture Compressor), meshopt | MIT / Apache-2.0 | converter: textures, meshes |
| vorbis_rs (libvorbis, aoTuV), lewton | BSD-3-Clause, MIT / Apache-2.0 | converter: car sounds |
