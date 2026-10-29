#!/usr/bin/env bash
set -euo pipefail
source_dir=${1:?Pass source directory}
helper=$(mktemp -d /tmp/ochat-glib-regression.XXXXXX)
mkdir -p "$helper/src"
cat > "$helper/Cargo.toml" <<EOF
[package]
name = "ochat-glib-security-regression"
version = "0.0.0"
edition = "2021"
[dependencies]
glib = { path = "$source_dir/vendor/glib-0.18.5" }
EOF
cat > "$helper/src/lib.rs" <<'RS'
#[cfg(test)]
mod tests {
    use glib::variant::ToVariant;
    #[test]
    fn optimized_string_iterator_all_paths() {
        let value = ["alpha", "βeta", "emoji 🛡", "delta"].to_variant();
        assert_eq!(value.array_iter_str().unwrap().collect::<Vec<_>>(), ["alpha", "βeta", "emoji 🛡", "delta"]);
        let mut mixed = value.array_iter_str().unwrap();
        assert_eq!(mixed.next(), Some("alpha"));
        assert_eq!(mixed.next_back(), Some("delta"));
        assert_eq!(mixed.next(), Some("βeta"));
        assert_eq!(mixed.next_back(), Some("emoji 🛡"));
        assert_eq!(mixed.next(), None);
        assert_eq!(value.array_iter_str().unwrap().nth(2), Some("emoji 🛡"));
        assert_eq!(value.array_iter_str().unwrap().nth_back(2), Some("βeta"));
        assert_eq!(value.array_iter_str().unwrap().last(), Some("delta"));
    }
}
RS
CARGO_TARGET_DIR=/opt/ochat-security-glib-target cargo test --manifest-path "$helper/Cargo.toml" --release
printf 'GLib security helper retained at %s\n' "$helper"
