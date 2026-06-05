#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/sources/fr.mangadex"
SOURCE_ID="fr.mangadex"
API_URL="${MANGADEX_API_URL:-https://api.mangadex.org}"
USER_AGENT="${MANGADEX_USER_AGENT:-Aidoku-Sources/1.0 (https://github.com/K0Konut/aidoku-Sources)}"

fetch() {
	local url="$1"
	curl -fsSL \
		--retry 2 \
		--retry-delay 1 \
		--connect-timeout 10 \
		--max-time 30 \
		-A "$USER_AGENT" \
		-H "Accept: application/json" \
		"$url"
}

require_contains() {
	local label="$1"
	local body="$2"
	local needle="$3"

	if ! grep -Fq "$needle" <<<"$body"; then
		echo "missing $label: $needle" >&2
		exit 1
	fi

	echo "ok $label"
}

echo "Checking live MangaDex API..."
ping_json="$(fetch "$API_URL/ping")"
search_json="$(fetch "$API_URL/manga?limit=1&availableTranslatedLanguage%5B%5D=fr&hasAvailableChapters=true&contentRating%5B%5D=safe&includes%5B%5D=cover_art")"

require_contains "api ping" "$ping_json" "pong"
require_contains "manga search result" "$search_json" "\"result\":\"ok\""
require_contains "manga search data" "$search_json" "\"data\":["

if ! command -v aidoku >/dev/null 2>&1; then
	echo "aidoku CLI is required for packaging and public build" >&2
	exit 1
fi

echo "Checking Rust source..."
(
	cd "$SOURCE_DIR"
	cargo fmt --check
	cargo check --target wasm32-unknown-unknown
)

echo "Packaging and verifying all local sources..."
build_files=()
while IFS= read -r manifest; do
	source_dir="${manifest%/Cargo.toml}"
	echo "Packaging $(basename "$source_dir")..."
	(
		cd "$source_dir"
		aidoku package
		aidoku verify package.aix
	)
	build_files+=("$source_dir/package.aix")
done < <(find "$ROOT_DIR/sources" -mindepth 2 -maxdepth 2 -name Cargo.toml -print | sort)

echo "Rebuilding public source list..."
TMP_PUBLIC="$(mktemp -d)"
trap 'rm -rf "$TMP_PUBLIC"' EXIT
aidoku build "${build_files[@]}" -o "$TMP_PUBLIC"

mkdir -p "$ROOT_DIR/public/icons" "$ROOT_DIR/public/sources"
cp "$TMP_PUBLIC/index.json" "$ROOT_DIR/public/index.json"
cp "$TMP_PUBLIC/index.min.json" "$ROOT_DIR/public/index.min.json"

for generated in "$TMP_PUBLIC/icons"/*.png; do
	[ -e "$generated" ] || continue
	base_name="$(basename "$generated")"
	target="$ROOT_DIR/public/icons/$base_name"
	if [[ "$base_name" == "$SOURCE_ID"-v*.png || ! -e "$target" ]]; then
		cp "$generated" "$target"
	fi
done

for generated in "$TMP_PUBLIC/sources"/*.aix; do
	[ -e "$generated" ] || continue
	base_name="$(basename "$generated")"
	target="$ROOT_DIR/public/sources/$base_name"
	if [[ "$base_name" == "$SOURCE_ID"-v*.aix || ! -e "$target" ]]; then
		cp "$generated" "$target"
	fi
done

echo "MangaDex verification complete."
