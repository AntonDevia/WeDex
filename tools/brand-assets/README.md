# wedex branding tools

The attached `logo-generator` skill guided the identity: three wordmark directions, selection, self-contained SVG, monochrome/inverse, compact symbol, and visual checks. Its validator was run on all ten SVG outputs.

Sources are pinned in `assets/fonts/medeo/sources.json`; original TTFs and OFL notices remain together. `generate.py` uses fontTools 4.68.0 to turn Medeo Sans Display glyphs into paths and combines them with custom w/x geometry. Rendering uses pinned `@resvg/resvg-js` 2.6.2. `preview.py` uses Pillow to compose the preview and size-check sheets and writes a Windows ICO.

Run from the project root:

```text
python tools/brand-assets/generate.py
node tools/brand-assets/render.mjs
python tools/brand-assets/preview.py
```

Python looks for fontTools in `.local/design-python`; install `fonttools==4.68.0` there, or provide it in the Python environment. Pillow is also required. Run `npm ci --ignore-scripts` in this directory for the locked renderer dependency. The output is `output/wedex/`.

Figma migration scripts and exact created/modified IDs are in `design/figma-branding.json`. The before-font snapshot is retained in `design/typography-before-medeo.json`. These scripts are native Figma Plugin API bodies called through `figma_local`; font migration input is split into bounded chunks.

The Rust app embeds the static fonts and PNG artwork using `include_bytes!` / `include_image!`, so it runs without a font installation. Windows/Figma uses the user-only installation prepared by `scripts/install-medeo-fonts.py`.
