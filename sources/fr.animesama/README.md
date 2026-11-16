# Anime-Sama

Aidoku source for the scans section of `https://anime-sama.to`.

Supports catalog/search pages (scans only), genre/status/sort filters, manga
details, chapter lists and reader pages.

Each scan panel of a work is its own entry: `one-piece` is the work's first
panel (colour), `one-piece/scan_noir-et-blanc/vf` the black and white one, and
spin-offs get their own entries too. English (VA) panels and JoJo's "all parts"
panel are skipped. Pages use the site's page-by-page folder (`<name> pp`) when
it exists.

The catalogue does not say which works have several panels, so
`src/variants.rs` lists them. Regenerate it when the site adds a spin-off or a
new version (the verification script fails when it is stale):

```sh
scripts/update-animesama-variants.py
```

## Build

```sh
aidoku package
```

From the repository root, the full verification/build flow is:

```sh
scripts/verify-animesama.sh
```

## Test Locally

```sh
aidoku serve package.aix
```
