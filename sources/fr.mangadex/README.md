# MangaDex FR

Aidoku source for French chapters on `https://mangadex.org`, backed by the
official MangaDex API.

Supports title search, status/demographic/content-rating filters, popular/new
listings, manga details, French chapter feeds and MangaDex@Home image pages.

## Build

```sh
aidoku package
```

From the repository root, the full verification/build flow is:

```sh
scripts/verify-mangadex.sh
```

## Test Locally

```sh
aidoku serve package.aix
```
