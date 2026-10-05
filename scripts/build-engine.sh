#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
engine_dir="$project_dir/vendor/radd"
revision=4fd4f59c9a442e6414c20ae3fd364a082e97f1ff
command -v cargo >/dev/null || { echo 'Install Rust first; see README.md.' >&2; exit 1; }
command -v git >/dev/null || { echo 'Install Git first.' >&2; exit 1; }
if [ ! -d "$engine_dir" ]; then
    mkdir -p "$project_dir/vendor"
    git clone --no-checkout https://github.com/pnnl/radd.git "$engine_dir"
    git -C "$engine_dir" checkout --detach "$revision"
fi
if [ "$(git -C "$engine_dir" rev-parse HEAD)" != "$revision" ]; then
    echo 'Unexpected RADD revision. Move vendor/radd aside and retry.' >&2
    exit 1
fi
cp "$project_dir/scripts/engine.Cargo.lock" "$engine_dir/Cargo.lock"
cargo build --manifest-path "$engine_dir/Cargo.toml" --release --locked --bin radd --config profile.release.lto=false --config profile.release.package.rapstone.opt-level=0
mkdir -p "$project_dir/tools"
cp "$engine_dir/target/release/radd" "$project_dir/tools/radd"
echo "Engine ready: $project_dir/tools/radd"

