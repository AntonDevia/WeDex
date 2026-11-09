# Third-party code and references

## Android integration

AndroidX GameActivity 4.4.0, AppCompat 1.7.1 and their AndroidX dependencies use Apache-2.0; the Kotlin runtime retains its Apache-2.0 license. Maven dependencies are declared in `android/app/build.gradle`. The APK embeds the Rust dependency inventory and available license texts, Medeo OFL, Font Awesome license and these notices under `assets/licenses/`. Android launch and notification icons derive from the application's wordmark and Font Awesome Free assets. No service tokens or user library are bundled.

## Vendored fingerprinting code (MIT)

Files under `src/fingerprinting/` originate from https://github.com/shazamio/shazamio-core at commit `700448e0cbae489e508299aa451a01d6299e2f48`.

- `algorithm.rs`: fingerprint algorithm retained; removed file/byte decoding, resampling entry points and their tests, because this application supplies PCM from CPAL. Imports and constants were adjusted; Rustfmt was applied.
- `hanning.rs`: upstream window constants.
- `signature_format.rs`: upstream signature encoder and its unit tests.
- `LICENSE-MIT`: complete upstream MIT license, copyright (c) 2024 dotX12.

ShazamIO's HTTP protocol was studied at https://github.com/shazamio/ShazamIO. Its Python package is not bundled or executed. The upstream fingerprinting implementation references the SongRec research project at https://github.com/marin-m/SongRec.

## References without copied implementation

- https://github.com/R3DCyclops/mp3_PLRKLR (GPL-3.0): examined to understand alternative-source search. No Python/GPL implementation was copied or translated into this codebase.
- https://github.com/MarshalX/yandex-music-api (LGPL-3.0): API routes and protocol behavior were consulted. This application independently implements HTTP/JSON/XML interactions; the Python library is not bundled.
- https://www.last.fm/api/show/track.getSimilar — recommendation data.
- https://developers.soundcloud.com/docs/api/guide and https://github.com/soundcloud/api — current SoundCloud API schema and migration notes.

## Rust dependencies

Cargo dependencies are recorded in `Cargo.lock`. `scripts/package.ps1` exports an inventory and available upstream license files into `dist/WeDex-0.1.0/licenses/`. They retain their respective licenses. This project's MIT license does not override dependency licenses.
## Medeo typography

Medeo Sans and Medeo Sans Display by The Medeo Project Authors, derived from Geist and Inter. Unmodified static TTF files are embedded in the application under SIL Open Font License 1.1. The license, copyright notices, upstream font log and pinned revision with SHA-256 hashes are in `assets/fonts/medeo/`.

Source: https://github.com/DNFDEVS/medeo-font, revision `72c9ca7cbd091f8b85ff2cee26e3dcc4e762f1a7`. The wedex wordmark uses Medeo Sans Display outlines with custom w/x geometry; it is supplied as font-independent SVG artwork.

## Interface assets

Application controls use Font Awesome Free Classic Solid icons by Fonticons, Inc. (CC BY 4.0). SVG sources, attribution, mapping and full upstream license are in `assets/fontawesome/`; `render-fontawesome.mjs` and `normalize_fontawesome.py` produce the embedded antialiased PNGs. Original Lucide references remain in `design/assets/` under ISC solely as design source material.

The six original Figma cover images are used for curated discovery seeds and isolated design previews. Source URLs and album/artist credits are recorded in `design/assets/sources.json`; production search, playback and playlists use the cover metadata returned by their providers. Album artwork retains its respective owners' rights.


### Service connection marks

The five transparent service marks were supplied by the user in `music_icons_transparent (5).zip`. Alpha bounds were cropped and normalized to a common visible extent, preserving proportions. Brand marks identify their respective services and retain their owners' rights. Provenance is in `assets/providers/sources.json`; older source SVGs and notices remain as design references.

Lyrics are fetched on demand from [LRCLIB](https://lrclib.net/docs.txt). No lyrics collection is bundled; preview lines are invented fixtures.

The Windows VPS installer uses [russh](https://github.com/warp-tech/russh) under Apache-2.0 for SSH password/key authentication and host-key verification. Its crypto dependency notices are included in the generated Rust license inventory. The bundled Linux server is built from `crates/wedex-cloud` and transferred through that SSH connection; no external installer script or binary download is executed on the VPS.
