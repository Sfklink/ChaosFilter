# ChaosFilter static site

This folder contains a **static front-end website** that explains the ChaosFilter project and can optionally **host Cargo API docs** for the `chaosfilter` crate.

## Open the site

- **Quick**: open `site/index.html`
- **Recommended (local server)**:

```powershell
cd site
./serve.ps1
```

Then open `http://localhost:5173`.

## Include Cargo API docs (optional)

To generate Rust docs and copy **`target/doc/chaosfilter`** into **`site/chaosfilter/`** (so the homepage can link to `chaosfilter/index.html`):

```powershell
./site/build-api-docs.ps1
```

On Linux/macOS:

```bash
./site/build-api-docs.sh
```

This runs:

- `cargo doc --no-deps -p chaosfilter` (builds under `target/doc/`)
- copies `target/doc/chaosfilter/` into `site/chaosfilter/`

Afterwards, the “Open API docs” button on the homepage will appear when served via a local server.

For **GitHub Pages**, copy `site/chaosfilter/` into `docs/chaosfilter/` before pushing (or run the script and sync that folder into `docs/`).
