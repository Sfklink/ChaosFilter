# ChaosFilter static site

This folder contains a **static front-end website** that explains the ChaosFilter project and can optionally **host Cargo API docs**.

## Open the site

- **Quick**: open `site/index.html`
- **Recommended (local server)**:

```powershell
cd site
./serve.ps1
```

Then open `http://localhost:5173`.

## Include Cargo API docs (optional)

To generate Rust docs and copy them into `site/api/` so the website can link them:

```powershell
./site/build-api-docs.ps1
```

On Linux/macOS:

```bash
./site/build-api-docs.sh
```

This runs:

- `cargo doc --no-deps` (builds to `target/doc`)
- copies `target/doc/*` into `site/api/`

Afterwards, the “Open API docs” button on the homepage will appear when served via a local server.

