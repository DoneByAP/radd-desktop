#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bash "$project_dir/scripts/build-engine.sh"
cargo build --manifest-path "$project_dir/Cargo.toml" --release --locked
bundle_dir="$project_dir/dist/RADD-Desktop-$(uname -s)-$(uname -m)"
mkdir -p "$bundle_dir"
cp "$project_dir/target/release/radd-desktop" "$project_dir/tools/radd" "$project_dir/README.md" "$project_dir/LICENSE" "$bundle_dir/"
cp -R "$project_dir/licenses" "$bundle_dir/"
cp -R "$project_dir/samples" "$bundle_dir/"
echo "Ready. Run: $bundle_dir/radd-desktop"

