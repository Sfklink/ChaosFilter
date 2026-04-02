# ChaosFilter static site

Marketing / overview pages live here: **`site/`** (`index.html`, `styles.css`). API docs are built in CI on pushes to `main` (see `.github/workflows/transfer_cargo_doc.yml`).

## Preview locally

- Open `site/index.html`, or use the **Live Server** extension with `site/` as the root, or:

```powershell
cd site
python -m http.server 8080
```

## `chaosfilter/index.html` link

The **Open API docs** button targets `chaosfilter/index.html` next to `index.html` (i.e. `site/chaosfilter/index.html` when the bundle is present). For a **local** check, run `cargo doc` and copy `target/doc/chaosfilter/` into `site/chaosfilter/`.
