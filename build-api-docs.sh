#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
site_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
out_dir="$site_dir/api"

command -v cargo >/dev/null 2>&1 || {
  echo "cargo not found on PATH" >&2
  exit 1
}

echo "Building Cargo docs (no deps)..."
(cd "$repo_root" && cargo doc --no-deps)

built_doc="$repo_root/target/doc"
if [ ! -d "$built_doc" ]; then
  echo "Expected docs at $built_doc but it doesn't exist." >&2
  exit 1
fi

echo "Copying docs to $out_dir ..."
rm -rf "$out_dir"
mkdir -p "$out_dir"
cp -R "$built_doc"/. "$out_dir"/

echo "Done. Open ./site/index.html and click 'Open API docs'."

