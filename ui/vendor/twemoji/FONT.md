# Native text-field font

OchatTwemoji.woff2 is a COLRv0 conversion of the Twemoji 17.0.3 SVG artwork in this directory. Artwork by Twitter and contributors, CC-BY 4.0; see LICENSE-GRAPHICS and ATTRIBUTION.md. The conversion adds native text shaping and does not replace the Unicode text stored or sent by Ochat.

Source artwork: https://github.com/jdecked/twemoji/tree/v17.0.3/assets/svg

Font compiler: https://github.com/googlefonts/nanoemoji (Apache-2.0, build-time only).

The font is generated from the local SVGs. It is included in the application and is never downloaded at runtime. Standard text fields retain native selection, IME, plain-text clipboard, and editing shortcuts. COLRv0 was selected for broader support than COLRv1; this release verifies Windows WebView2's Chromium engine. Linux and macOS text rendering still require platform checks.

Rebuild from the project root in a Python virtual environment with nanoemoji 0.16.0, fonttools 4.67.0, brotli 1.2.1 and uharfbuzz 0.57.0 installed. Make the environment's Scripts/bin directory available on PATH:

```text
nanoemoji --build_dir=local-data/twemoji-font-build --noexec_ninja scripts/fonts/twemoji.toml
ninja -C local-data/twemoji-font-build -j 8
python scripts/fonts/finish-twemoji.py
```

The finishing script adds longest-match sequence shaping, qualified/unqualified variants and emoji presentation selectors, then verifies each source sequence with HarfBuzz before packaging WOFF2. It writes a font report containing the sequence count, file size and SHA-256 beside the temporary build output. The report is retained with the release validation artifacts.
