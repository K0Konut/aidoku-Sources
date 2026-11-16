# Anime-Sama

Aidoku source for the scans section of `https://anime-sama.to`.

Supports catalog/search pages (scans only), genre/status/sort filters, manga
details, chapter lists (one scanlator per scan panel, e.g. colour / black and
white) and reader pages.

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
