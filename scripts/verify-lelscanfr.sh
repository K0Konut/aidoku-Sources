#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/sources/fr.lelscanfr"
BASE_URL="${LELSCANFR_BASE_URL:-https://www.lelscanfr.com}"
USER_AGENT="${LELSCANFR_USER_AGENT:-Mozilla/5.0 (Aidoku)}"

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
	local html="$2"
	local needle="$3"

	if ! grep -Fq "$needle" <<<"$html"; then
		echo "missing $label: $needle" >&2
		exit 1
	fi

	echo "ok $label"
}

require_regex() {
	local label="$1"
	local html="$2"
	local pattern="$3"

	if ! grep -Eq "$pattern" <<<"$html"; then
		echo "missing $label: $pattern" >&2
		exit 1
	fi

	echo "ok $label"
}

echo "Checking live LelscanFR selectors..."
manga_html="$(fetch "$BASE_URL/manga")"
home_html="$(fetch "$BASE_URL")"

require_contains "manga cards" "$manga_html" "id=\"card-real\""
require_contains "type filter" "$manga_html" "name=\"type\""
require_contains "status filter" "$manga_html" "name=\"status\""
require_contains "genre filter" "$manga_html" "name=\"genre[]\""
require_contains "manga pagination" "$manga_html" "pagination-link"
require_regex "manga detail links" "$manga_html" "href=\"[^\"]*/manga/[^/\"?#]+\""

require_contains "popular listing" "$home_html" "id=\"popular-cards\""
require_contains "latest listing" "$home_html" "id=\"latest-cards\""
require_contains "recent chapters heading" "$home_html" "Chapitres récents"
require_regex "recent chapter links" "$home_html" "href=\"[^\"]*/manga/[^/\"?#]+/[0-9][^\"?#]*\""

if ! command -v aidoku >/dev/null 2>&1; then
	echo "aidoku CLI is required for packaging and public build" >&2
	exit 1
fi

echo "Packaging and verifying source..."
(
	cd "$SOURCE_DIR"
	aidoku package
	aidoku verify package.aix
)

echo "Rebuilding public source list..."
aidoku build "$SOURCE_DIR/package.aix" -o "$ROOT_DIR/public"

echo "LelscanFR verification complete."
