#![cfg_attr(not(test), no_std)]

extern crate alloc;

use aidoku::{
    helpers::uri::encode_uri_component,
    imports::{
        html::{Document, Element, Html},
        net::Request,
    },
    prelude::*,
    Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
    MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};

const BASE_URL: &str = "https://anime-sama.to";
const USER_AGENT: &str = "Mozilla/5.0 (Aidoku)";

struct AnimeSama;

impl Source for AnimeSama {
    fn new() -> Self {
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        catalog_page(&catalog_url(query, page, &filters, None))
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let url = manga_url(&manga.key);
        let raw = get_string(&url)?;

        if needs_details {
            let html = Html::parse_with_url(&raw, &url)?;
            if let Some(title) = select_text(&html, "h1#titreOeuvre") {
                manga.title = title;
            }
            manga.cover = html
                .select_first(".oe-affiche img")
                .and_then(|element| element.attr("abs:src"))
                .or(manga.cover);
            manga.authors = info_value(&html, "Auteur")
                .or_else(|| info_value(&html, "Créateur"))
                .map(|value| value.split(',').map(|a| a.trim().to_string()).collect());
            manga.description = select_text(&html, "#synopsisText");
            manga.tags = html.select(".oe-genres a.genre-pill").map(|pills| {
                pills
                    .filter_map(|pill| pill.text())
                    .collect::<Vec<String>>()
            });
            manga.status = match info_value(&html, "Statut").as_deref() {
                Some("En cours") => MangaStatus::Ongoing,
                Some("Terminé") => MangaStatus::Completed,
                _ => MangaStatus::Unknown,
            };
            manga.content_rating = content_rating(manga.tags.as_ref());
            manga.viewer = viewer(manga.tags.as_ref(), false);
            manga.url = Some(url.clone());
        }

        if needs_chapters {
            manga.chapters = Some(fetch_chapters(&url, &raw)?);
        }

        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let Some((oeuvre, folder)) = chapter.key.rsplit_once('|') else {
            bail!("Invalid chapter key");
        };
        let Some(count) = fetch_page_counts(oeuvre)?.remove(folder) else {
            bail!("Chapitre indisponible sur Anime-Sama");
        };
        let oeuvre = encode_uri_component(oeuvre);

        Ok((1..=count)
            .map(|page| Page {
                content: PageContent::url(format!(
                    "{BASE_URL}/s2/scans/{oeuvre}/{folder}/{page}.jpg"
                )),
                ..Default::default()
            })
            .collect())
    }
}

impl ListingProvider for AnimeSama {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        let sort = (listing.id == "new").then_some("ajout");
        catalog_page(&catalog_url(None, page, &[], sort))
    }
}

impl ImageRequestProvider for AnimeSama {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        Ok(Request::get(url)?
            .header("Referer", BASE_URL)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "image/*"))
    }
}

fn request(url: &str) -> Result<Request> {
    Ok(Request::get(url)?
        .header("User-Agent", USER_AGENT)
        .header("Referer", BASE_URL))
}

fn get_string(url: &str) -> Result<String> {
    request(url)?.string()
}

fn manga_url(key: &str) -> String {
    format!("{BASE_URL}/catalogue/{key}/")
}

fn catalog_url(
    query: Option<String>,
    page: i32,
    filters: &[FilterValue],
    sort: Option<&str>,
) -> String {
    let mut params = vec![String::from("type%5B%5D=Scans")];
    let mut push = |key: &str, value: &str| {
        if !value.trim().is_empty() {
            params.push(format!("{key}={}", encode_uri_component(value.trim())));
        }
    };

    if let Some(query) = &query {
        push("search", query);
    }
    for filter in filters {
        match filter {
            FilterValue::MultiSelect { id, included, .. } if id == "genre" => {
                for value in included {
                    push("genre%5B%5D", value);
                }
            }
            FilterValue::Select { id, value } if id == "status" => push("current%5B%5D", value),
            FilterValue::Select { id, value } if id == "sort" => push("tri", value),
            _ => {}
        }
    }
    if let Some(sort) = sort {
        push("tri", sort);
    }
    if page > 1 {
        push("page", &page.to_string());
    }

    format!("{BASE_URL}/catalogue/?{}", params.join("&"))
}

fn catalog_page(url: &str) -> Result<MangaPageResult> {
    let html = request(url)?.html()?;
    let mut entries: Vec<Manga> = Vec::new();

    for card in html
        .select("#list_catalog .as-carte-oeuvre")
        .into_iter()
        .flatten()
    {
        let Some(key) = card.attr("data-url").and_then(|url| manga_key(&url)) else {
            continue;
        };
        if entries.iter().any(|entry| entry.key == key) {
            continue;
        }
        let title = card
            .attr("data-nom")
            .or_else(|| card.select_first(".as-carte-titre").and_then(|e| e.text()))
            .unwrap_or_else(|| key.clone());
        let is_webtoon = card.select_first(".as-badge-webtoon").is_some();

        entries.push(Manga {
            url: Some(manga_url(&key)),
            cover: card
                .select_first(".as-carte-image img")
                .and_then(|img| img.attr("abs:src")),
            viewer: viewer(None, is_webtoon),
            key,
            title,
            ..Default::default()
        });
    }

    Ok(MangaPageResult {
        entries,
        has_next_page: html.select_first("#list_pagination a[rel=next]").is_some(),
    })
}

/// "/catalogue/one-piece/" -> "one-piece"
fn manga_key(url: &str) -> Option<String> {
    let rest = url.split("/catalogue/").nth(1)?;
    let key = rest.split(['/', '?', '#']).next()?;
    (!key.is_empty()).then(|| key.to_string())
}

fn select_text(html: &Document, selector: &str) -> Option<String> {
    html.select_first(selector)
        .and_then(|element| element.text())
        .filter(|text| !text.is_empty())
}

/// Value of a `<dt>label</dt><dd>value</dd>` row. The manga block comes after the anime
/// block, so the last match wins.
fn info_value(html: &Document, label: &str) -> Option<String> {
    html.select("dl.oe-infos dt")?
        .filter(|dt| dt.text().as_deref() == Some(label))
        .filter_map(|dt: Element| dt.next().and_then(|dd| dd.text()))
        .last()
}

fn viewer(tags: Option<&Vec<String>>, is_webtoon: bool) -> Viewer {
    let webtoonish = |tag: &String| matches!(tag.as_str(), "Webcomic" | "Manhwa" | "Manhua");
    if is_webtoon || tags.is_some_and(|tags| tags.iter().any(webtoonish)) {
        Viewer::Webtoon
    } else {
        Viewer::RightToLeft
    }
}

fn content_rating(tags: Option<&Vec<String>>) -> ContentRating {
    let suggestive = |tag: &String| matches!(tag.as_str(), "Ecchi" | "Mature" | "Yaoi" | "Yuri");
    if tags.is_some_and(|tags| tags.iter().any(suggestive)) {
        ContentRating::Suggestive
    } else {
        ContentRating::Safe
    }
}

fn fetch_page_counts(oeuvre: &str) -> Result<BTreeMap<String, u32>> {
    let url = format!(
        "{BASE_URL}/s2/scans/get_nb_chap_et_img.php?oeuvre={}",
        encode_uri_component(oeuvre)
    );
    // Unknown works answer 200 with {"error": "..."}, which fails to parse as a map.
    Ok(request(&url)?
        .json_owned::<BTreeMap<String, u32>>()
        .unwrap_or_default())
}

/// One manga page can hold several scan panels (e.g. colour and black & white One Piece),
/// each with its own reader page, work name and chapter list.
fn fetch_chapters(manga_url: &str, raw: &str) -> Result<Vec<Chapter>> {
    let panels = parse_scan_panels(raw);
    let mut chapters = Vec::new();

    for (panel_name, path) in &panels {
        let reader_url = format!("{manga_url}{}/", path.trim_matches('/'));
        let reader = get_string(&reader_url)?;
        let Some(oeuvre) = reader_title(&reader) else {
            continue;
        };
        let counts = fetch_page_counts(&oeuvre)?;
        let labels = ChapterLabels::parse(&reader);
        let last = counts.keys().filter_map(|k| k.parse::<u32>().ok()).max();

        for folder in (1..=last.unwrap_or(0)).rev() {
            if !counts.contains_key(&folder.to_string()) {
                continue;
            }
            let (title, chapter_number) = labels.get(folder);
            chapters.push(Chapter {
                key: format!("{oeuvre}|{folder}"),
                title,
                chapter_number,
                scanlators: (panels.len() > 1).then(|| vec![panel_name.clone()]),
                url: Some(reader_url.clone()),
                language: Some("fr".to_string()),
                ..Default::default()
            });
        }
    }

    Ok(chapters)
}

/// `panneauScan("Scans (couleur)", "scan/vf");` -> ("Scans (couleur)", "scan/vf").
/// Skips commented-out panels and the "nom"/"url" template examples.
fn parse_scan_panels(raw: &str) -> Vec<(String, String)> {
    let mut panels: Vec<(String, String)> = Vec::new();

    for line in raw.lines().map(str::trim) {
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with("<!--") {
            continue;
        }
        let Some(start) = line.find("panneauScan(") else {
            continue;
        };
        let args = call_args(&line[start + "panneauScan(".len()..]).map(|(args, _)| args);
        if let Some([name, path]) = args.as_deref() {
            if name != "nom" && !panels.iter().any(|(_, p)| p == path) {
                panels.push((name.clone(), path.clone()));
            }
        }
    }

    panels
}

/// The reader script sends `#titreOeuvre`'s innerHTML to the API, so keep it raw.
fn reader_title(reader: &str) -> Option<String> {
    let start = reader.find("id=\"titreOeuvre\"")?;
    let rest = &reader[start..];
    let rest = &rest[rest.find('>')? + 1..];
    let title = rest[..rest.find('<')?].trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Chapter names of the reader dropdown. Option N of the dropdown reads folder N, and
/// the site rebuilds the dropdown from calls like:
/// `resetListe(); newSP(0); creerListe(1, 262); newSP(262.5); newSP("Épilogue"); finirListe(272);`
#[derive(Default)]
struct ChapterLabels {
    labels: Vec<(Option<String>, Option<f32>)>,
    tail_start: Option<f32>,
}

impl ChapterLabels {
    fn parse(reader: &str) -> Self {
        let mut result = Self::default();
        let Some(script) = custom_list_script(reader) else {
            return result;
        };
        let mut rest = script.as_str();

        while let Some((name, args, next)) = next_call(rest) {
            rest = next;
            let numbers: Vec<Option<f32>> = args.iter().map(|a| a.parse::<f32>().ok()).collect();
            match (name, numbers.as_slice()) {
                ("creerListe", [Some(from), Some(to)]) => {
                    for n in (*from as u32)..=(*to as u32) {
                        result.labels.push((None, Some(n as f32)));
                    }
                }
                ("newSP", [Some(n)]) => result.labels.push((None, Some(*n))),
                ("newSP" | "newSPF", [_]) => result.labels.push((Some(args[0].clone()), None)),
                ("finirListe", [Some(from)]) => result.tail_start = Some(*from),
                _ => {}
            }
        }

        result
    }

    fn get(&self, folder: u32) -> (Option<String>, Option<f32>) {
        let index = folder as usize - 1;
        if let Some(label) = self.labels.get(index) {
            return label.clone();
        }
        let start = self.tail_start.unwrap_or(self.labels.len() as f32 + 1.0);
        (None, Some(start + (index - self.labels.len()) as f32))
    }
}

/// The page's own `<script>` that calls `resetListe()`, with comments stripped
/// (it contains a commented usage example).
fn custom_list_script(reader: &str) -> Option<String> {
    let call = reader.find("resetListe(")?;
    let start = reader[..call].rfind("<script")?;
    let end = call + reader[call..].find("</script>")?;
    let mut script = String::new();
    let mut rest = &reader[start..end];

    while let Some(open) = rest.find("/*") {
        script.push_str(&rest[..open]);
        rest = rest[open..]
            .find("*/")
            .map_or("", |close| &rest[open + close + 2..]);
    }
    script.push_str(rest);

    Some(
        script
            .lines()
            .map(|line| line.split("//").next().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Finds the next list-building call and returns (name, args, rest after the call).
fn next_call(script: &str) -> Option<(&'static str, Vec<String>, &str)> {
    const NAMES: [&str; 4] = ["creerListe", "newSPF", "newSP", "finirListe"];
    let (start, name) = NAMES
        .iter()
        .filter_map(|name| {
            script
                .match_indices(name)
                .find(|(i, _)| script[i + name.len()..].starts_with('('))
                .map(|(i, _)| (i, *name))
        })
        .min_by_key(|(i, _)| *i)?;
    let rest = &script[start + name.len() + 1..];
    let (args, end) = call_args(rest)?;
    Some((name, args, &rest[end..]))
}

/// Parses `"a", 'b', 3)` into ["a", "b", "3"] plus the byte offset after `)`.
fn call_args(input: &str) -> Option<(Vec<String>, usize)> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for (i, c) in input.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, ',') => args.push(core::mem::take(&mut current).trim().to_string()),
            (None, ')') => {
                args.push(current.trim().to_string());
                return Some((args, i + 1));
            }
            (None, c) => current.push(c),
        }
    }

    None
}

register_source!(AnimeSama, ListingProvider, ImageRequestProvider);
