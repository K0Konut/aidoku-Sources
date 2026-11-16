#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/sources/fr.animesama"
SOURCE_ID="fr.animesama"
BASE_URL="${ANIMESAMA_BASE_URL:-https://anime-sama.to}"
USER_AGENT="${ANIMESAMA_USER_AGENT:-Mozilla/5.0 (Aidoku)}"

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

echo "Checking live Anime-Sama selectors..."
catalog_html="$(fetch "$BASE_URL/catalogue/?type%5B%5D=Scans")"
search_html="$(fetch "$BASE_URL/catalogue/?type%5B%5D=Scans&search=jujutsu%20kaisen")"
detail_html="$(fetch "$BASE_URL/catalogue/one-piece/")"
reader_html="$(fetch "$BASE_URL/catalogue/jujutsu-kaisen/scan/vf/")"
api_json="$(fetch "$BASE_URL/s2/scans/get_nb_chap_et_img.php?oeuvre=Jujutsu%20Kaisen")"

require_contains "catalog container" "$catalog_html" "id=\"list_catalog\""
require_contains "catalog cards" "$catalog_html" "as-carte-oeuvre\" data-url=\"/catalogue/"
require_contains "catalog pagination" "$catalog_html" "rel=\"next\""
require_contains "catalog genre filter" "$catalog_html" "name=\"genre[]\""
require_contains "search result" "$search_html" "data-url=\"/catalogue/jujutsu-kaisen/\""

require_contains "detail title" "$detail_html" "id=\"titreOeuvre\""
require_contains "detail synopsis" "$detail_html" "id=\"synopsisText\""
require_contains "detail infos" "$detail_html" "class=\"oe-infos\""
require_regex "detail scan panels" "$detail_html" "panneauScan\\(\"Scans \\(couleur\\)\", \"scan/vf\"\\)"

require_contains "reader title" "$reader_html" "id=\"titreOeuvre\""
require_contains "reader chapter list script" "$reader_html" "creerListe(1, 262);newSP(262.5);"
require_regex "page count api" "$api_json" "^\\{\"1\":[0-9]+"
# Folder names keep the title's trailing spaces ("Blue Lock  ").
require_contains "reader raw title" "$(fetch "$BASE_URL/catalogue/blue-lock/scan/vf/")" "class=\"ct-titre\">Blue Lock  <"
require_regex "page count api raw title" "$(fetch "$BASE_URL/s2/scans/get_nb_chap_et_img.php?oeuvre=Blue%20Lock%20%20")" "^\\{\"1\":[0-9]+"

require_contains "detail webtoon badge" "$(fetch "$BASE_URL/catalogue/solo-leveling/")" "class=\"type-badge webtoon\""
require_regex "page-by-page api" "$(fetch "$BASE_URL/s2/scans/get_nb_chap_et_img.php?oeuvre=Dragon%20Ball%20pp")" "^\\{\"1\":[0-9]+"

echo "Checking Anime-Sama variants table..."
python3 -I "$ROOT_DIR/scripts/update-animesama-variants.py" --check

if ! curl -fsS -o /dev/null -A "$USER_AGENT" -e "$BASE_URL" "$BASE_URL/s2/scans/Jujutsu%20Kaisen/1/1.jpg"; then
	echo "missing page image" >&2
	exit 1
fi
echo "ok page image"

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

echo "Anime-Sama verification complete."
