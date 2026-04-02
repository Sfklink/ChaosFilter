#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
site_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
built_doc="$repo_root/target/doc"
crate_doc="$built_doc/chaosfilter"
out_dir="$site_dir/chaosfilter"

command -v cargo >/dev/null 2>&1 || {
  echo "cargo not found on PATH" >&2
  exit 1
}

echo "Building Cargo docs (no deps)..."
(cd "$repo_root" && cargo doc --no-deps -p chaosfilter)

if [ ! -d "$crate_doc" ]; then
  echo "Expected crate docs at $crate_doc — run cargo doc --no-deps from the workspace root." >&2
  exit 1
fi

echo "Copying chaosfilter docs to $out_dir ..."
rm -rf "$out_dir"
cp -R "$crate_doc" "$out_dir"

echo "Done. Open ./site/index.html and click 'Open API docs'."
