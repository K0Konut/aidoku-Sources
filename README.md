# Aidoku Sources

Personal Aidoku source repository.

Aidoku sources are Rust projects compiled to WebAssembly packages (`.aix`). Each
source scrapes or calls a website/API, then exposes manga, chapters and page
URLs in the format Aidoku expects.

## Local Tooling

Install the Rust toolchain, the WebAssembly target, then the Aidoku CLI:

```sh
rustup install stable
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
```

This workspace currently does not include Rust or `aidoku`; install those before
building packages locally.

## Repository Layout

```text
sources/
  fr.lelscanfr/
    Cargo.toml
    src/lib.rs
    res/filters.json
    res/source.json
    res/icon.png
```

The important file is `res/source.json`. It defines the source id, display name,
version, supported languages, content rating and site URL.

## Creating A Source

From the repository root:

```sh
aidoku init sources/fr.example
cd sources/fr.example
aidoku package
```

The package command produces `package.aix`. During development, you can serve it
to Aidoku on a device connected to the same network:

```sh
aidoku serve package.aix
```

## Included Sources

- `fr.lelscanfr`: LelscanFR, French manga scans from
  `https://www.lelscanfr.com`

## What A Source Implements

A basic source needs to support:

- search/list manga results
- fetch manga details and chapters
- fetch page image URLs for a chapter

Most scan sites need CSS selectors for title, cover, chapter links and page
images. If the site exposes JSON/API endpoints, prefer those over scraping HTML.

## Publishing

For public installation in Aidoku, build each source into an `.aix` package and
host a source list (`index.min.json`) plus the package files through GitHub
Pages or another static host.

The current community repository URL format is:

```text
https://example.github.io/aidoku-sources/index.min.json
```

## Notes

Only build sources for sites you are allowed to access and automate. Do not
bypass authentication, paywalls, DRM, or anti-bot protections.
