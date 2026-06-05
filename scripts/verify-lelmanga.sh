#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/sources/fr.lelmanga"
SOURCE_ID="fr.lelmanga"
BASE_URL="${LELMANGA_BASE_URL:-https://www.lelmanga.com}"
USER_AGENT="${LELMANGA_USER_AGENT:-Mozilla/5.0 (Aidoku)}"

fetch() {
	local url="$1"
	curl -fsSL \
		--retry 2 \
		--retry-delay 1 \
		--connect-timeout 10 \
		--max-time 30 \
		-A "$USER_AGENT" \
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

require_regex() {
	local label="$1"
	local body="$2"
	local pattern="$3"

	if ! grep -Eq "$pattern" <<<"$body"; then
		echo "missing $label: $pattern" >&2
		exit 1
	fi

	echo "ok $label"
}

echo "Checking live Lelmanga selectors..."
catalog_html="$(fetch "$BASE_URL/manga")"
search_html="$(fetch "$BASE_URL/?s=one%20piece")"
detail_html="$(fetch "$BASE_URL/manga/one-piece")"
chapter_url="$(grep -Eo 'https://www\.lelmanga\.com/one-piece-[^"]+' <<<"$detail_html" | head -n 1 || true)"

if [ -z "$chapter_url" ]; then
	echo "missing latest One Piece chapter url" >&2
	exit 1
fi

chapter_html="$(fetch "$chapter_url")"

require_contains "catalog cards" "$catalog_html" "class=\"bsx\""
require_contains "catalog genre filter" "$catalog_html" "name=\"genre[]\""
require_contains "catalog status filter" "$catalog_html" "name=\"status\""
require_contains "catalog type filter" "$catalog_html" "name=\"type\""
require_contains "catalog pagination" "$catalog_html" "page-numbers"
require_regex "catalog manga links" "$catalog_html" "href=\"[^\"]*/manga/[^/\"?#]+\""

require_contains "search result" "$search_html" "/manga/one-piece"
require_contains "detail title" "$detail_html" "class=\"entry-title\""
require_contains "detail chapters" "$detail_html" "id=\"chapterlist\""
require_contains "detail chapter labels" "$detail_html" "class=\"chapternum\""
require_contains "detail chapter dates" "$detail_html" "class=\"chapterdate\""

require_contains "reader area" "$chapter_html" "id=\"readerarea\""
require_contains "reader script" "$chapter_html" "ts_reader.run"
require_regex "reader images" "$chapter_html" "wp-content/uploads/[0-9]{4}/[0-9]{2}/[^\"']+\\.(webp|jpg|jpeg|png)"

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

echo "Lelmanga verification complete."
