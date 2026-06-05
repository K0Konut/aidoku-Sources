# MangaDex FR

Aidoku source for French chapters on `https://mangadex.org`, backed by the
official MangaDex API.

Supports title search, status/demographic/content-rating filters, popular/new
listings, manga details, French chapter feeds and MangaDex@Home image pages.
External MangaDex chapters, such as Manga Plus links, are shown with a text
page explaining that they are hosted outside MangaDex.
Titles with no readable French feed show a placeholder chapter explaining that
MangaDex has no French pages for the entry.

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
