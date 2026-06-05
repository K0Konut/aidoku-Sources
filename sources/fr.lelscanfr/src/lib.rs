#![no_std]

extern crate alloc;

use aidoku::{
    helpers::uri::encode_uri,
    imports::{
        html::{Document, Element, ElementList},
        net::Request,
    },
    prelude::*,
    Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
    MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

const BASE_URL: &str = "https://www.lelscanfr.com";
const USER_AGENT: &str = "Mozilla/5.0 (Aidoku)";
const MANGA_CARD_LINK_SELECTOR: &str = "div[id='card-real'] a[href*='/manga/']";
const POPULAR_CARD_LINK_SELECTOR: &str = "#popular-cards div[id='card-real'] a[href*='/manga/']";
const LATEST_CARD_LINK_SELECTOR: &str = "#latest-cards div[id='card-real'] a[href*='/manga/']";

struct LelscanFr;

impl Source for LelscanFr {
    fn new() -> Self {
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let url = search_url(query, page, &filters);
        let html = get_html(&url)?;
        let entries = parse_manga_cards(html.select(MANGA_CARD_LINK_SELECTOR), false);

        Ok(MangaPageResult {
            entries,
            has_next_page: has_next_page(&html, page),
        })
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let url = manga_url(&manga.key, 1);
        let html = get_html(&url)?;

        if needs_details {
            if let Some(cover) = html
                .select_first("main section img[src*='/storage/covers/']")
                .and_then(|element| element.attr("abs:src"))
            {
                manga.cover = Some(cover);
            }

            if manga.title.is_empty() {
                if let Some(title) = html
                    .select_first("main section img[src*='/storage/covers/']")
                    .and_then(|element| element.attr("alt"))
                {
                    manga.title = title;
                }
            }

            manga.description = html
                .select_first("#description")
                .and_then(|element| element.text());
            manga.authors = metadata_value(&html, "Auteur").map(|value| vec![value]);
            manga.artists = metadata_value(&html, "Artiste").map(|value| vec![value]);
            manga.tags = parse_tags(&html);
            manga.status = parse_status(&html);
            manga.url = Some(url.clone());
            manga.content_rating = content_rating_from_tags(manga.tags.as_ref());
            manga.viewer = Viewer::RightToLeft;
        }

        if needs_chapters {
            manga.chapters = Some(fetch_chapters(&manga.key, html)?);
        }

        Ok(manga)
    }

    fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let url = chapter
            .url
            .clone()
            .unwrap_or_else(|| format!("{}/manga/{}/{}", BASE_URL, manga.key, chapter.key));
        let html = get_html(&url)?;
        let mut pages = Vec::new();

        if let Some(images) = html.select("#chapter-container img.chapter-image") {
            for image in images {
                let Some(url) = image.attr("abs:data-src").or_else(|| image.attr("abs:src")) else {
                    continue;
                };

                pages.push(Page {
                    content: PageContent::url(url),
                    ..Default::default()
                });
            }
        }

        Ok(pages)
    }
}

impl ListingProvider for LelscanFr {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        match listing.id.as_str() {
            "all" => self.get_search_manga_list(None, page, Vec::new()),
            "popular" => get_home_listing(POPULAR_CARD_LINK_SELECTOR, page),
            "latest" => get_home_listing(LATEST_CARD_LINK_SELECTOR, page),
            "recent_chapters" => get_recent_chapters_listing(page),
            _ => self.get_search_manga_list(None, page, Vec::new()),
        }
    }
}

impl ImageRequestProvider for LelscanFr {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        Ok(Request::get(url)?
            .header("Referer", BASE_URL)
            .header("User-Agent", USER_AGENT))
    }
}

fn get_html(url: &str) -> Result<Document> {
    Ok(Request::get(url)?
        .header("User-Agent", USER_AGENT)
        .header("Referer", BASE_URL)
        .html()?)
}

fn get_home_listing(selector: &str, page: i32) -> Result<MangaPageResult> {
    if page > 1 {
        return Ok(MangaPageResult {
            entries: Vec::new(),
            has_next_page: false,
        });
    }

    let html = get_html(BASE_URL)?;
    Ok(MangaPageResult {
        entries: parse_manga_cards(html.select(selector), false),
        has_next_page: false,
    })
}

fn get_recent_chapters_listing(page: i32) -> Result<MangaPageResult> {
    let page = page.max(1);
    let html = get_html(&home_url(page))?;
    let entries = find_section_by_heading(&html, "Chapitres récents")
        .map(|section| parse_manga_cards(section.select(MANGA_CARD_LINK_SELECTOR), true))
        .unwrap_or_default();

    Ok(MangaPageResult {
        entries,
        has_next_page: has_next_page(&html, page),
    })
}

fn parse_manga_cards(cards: Option<ElementList>, include_chapters: bool) -> Vec<Manga> {
    let mut entries = Vec::new();

    if let Some(cards) = cards {
        for card in cards {
            let Some(url) = card.attr("abs:href") else {
                continue;
            };

            let Some(key) = manga_key_from_url(&url) else {
                continue;
            };

            let image = card.select_first("img");
            let title = image
                .as_ref()
                .and_then(|element| element.attr("alt"))
                .or_else(|| card.select_first("h2").and_then(|element| element.text()))
                .unwrap_or_else(|| key_to_title(&key));
            let cover = image.and_then(|element| {
                element
                    .attr("abs:data-src")
                    .or_else(|| element.attr("abs:src"))
            });
            let container = card.parent().and_then(|element| element.parent());
            let tags = container.as_ref().and_then(parse_tags_from_element);
            let content_rating = content_rating_from_tags(tags.as_ref());
            let chapters = if include_chapters {
                container
                    .as_ref()
                    .and_then(|element| parse_chapter_links(&key, element))
            } else {
                None
            };

            entries.push(Manga {
                key,
                title,
                cover,
                url: Some(url),
                tags,
                chapters,
                content_rating,
                viewer: Viewer::RightToLeft,
                ..Default::default()
            });
        }
    }

    entries
}

fn search_url(query: Option<String>, page: i32, filters: &[FilterValue]) -> String {
    let page = page.max(1);
    let mut params = Vec::new();

    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        push_query_param(&mut params, "title", &query);
    }

    for filter in filters {
        match filter {
            FilterValue::Select { id, value } => push_filter_param(&mut params, id, value),
            FilterValue::MultiSelect { id, included, .. } => {
                for value in included {
                    push_filter_param(&mut params, id, value);
                }
            }
            _ => {}
        }
    }

    if page > 1 {
        params.push(format!("page={}", page));
    }

    if params.is_empty() {
        format!("{}/manga", BASE_URL)
    } else {
        format!("{}/manga?{}", BASE_URL, params.join("&"))
    }
}

fn push_filter_param(params: &mut Vec<String>, id: &str, value: &str) {
    match id {
        "genre" => push_query_param(params, "genre[]", value),
        "status" | "type" => push_query_param(params, id, value),
        _ => {}
    }
}

fn push_query_param(params: &mut Vec<String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        params.push(format!("{}={}", encode_uri(key), encode_uri(value)));
    }
}

fn manga_url(key: &str, page: i32) -> String {
    if page <= 1 {
        format!("{}/manga/{}", BASE_URL, key)
    } else {
        format!("{}/manga/{}?page={}", BASE_URL, key, page)
    }
}

fn home_url(page: i32) -> String {
    if page <= 1 {
        BASE_URL.to_string()
    } else {
        format!("{}?page={}", BASE_URL, page)
    }
}

fn manga_key_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let marker = "/manga/";
    let start = path.find(marker)? + marker.len();
    let rest = &path[start..];
    let key = rest.split('/').next().unwrap_or_default();

    if key.is_empty() || key == "random" {
        None
    } else {
        Some(key.to_string())
    }
}

fn chapter_key_from_url(manga_key: &str, url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let marker = format!("/manga/{}/", manga_key);
    let start = path.find(&marker)? + marker.len();
    let key = path[start..].split('/').next().unwrap_or_default();

    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn key_to_title(key: &str) -> String {
    key.split('-')
        .map(capitalize)
        .collect::<Vec<String>>()
        .join(" ")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    first.to_uppercase().chain(chars).collect::<String>()
}

fn find_section_by_heading(html: &Document, heading: &str) -> Option<Element> {
    let headings = html.select("main h2")?;

    for element in headings {
        let Some(text) = element.text() else {
            continue;
        };

        if text.trim() != heading {
            continue;
        }

        let mut current = element;
        loop {
            if current.tag_name().as_deref() == Some("section") {
                return Some(current);
            }

            current = current.parent()?;
        }
    }

    None
}

fn parse_status(html: &Document) -> MangaStatus {
    let Some(status) = metadata_value(html, "Statut") else {
        return MangaStatus::Unknown;
    };

    let normalized = status.to_lowercase();
    if normalized.contains("cours") {
        MangaStatus::Ongoing
    } else if normalized.contains("termin") || normalized.contains("complete") {
        MangaStatus::Completed
    } else if normalized.contains("pause") || normalized.contains("hiatus") {
        MangaStatus::Hiatus
    } else {
        MangaStatus::Unknown
    }
}

fn metadata_value(html: &Document, label: &str) -> Option<String> {
    let rows = html.select("main p")?;

    for row in rows {
        let text = row.text()?;
        if text.starts_with(label) {
            let value = text[label.len()..].trim().trim_start_matches(':').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

fn parse_tags(html: &Document) -> Option<Vec<String>> {
    parse_tags_from_links(html.select("main a[href*='genre=']"))
}

fn parse_tags_from_element(element: &Element) -> Option<Vec<String>> {
    parse_tags_from_links(element.select("a[href*='genre=']"))
}

fn parse_tags_from_links(links: Option<ElementList>) -> Option<Vec<String>> {
    let links = links?;
    let mut tags = Vec::new();

    for link in links {
        let Some(tag) = link
            .select_first("span")
            .and_then(|element| element.text())
            .or_else(|| link.text())
            .and_then(clean_tag)
        else {
            continue;
        };

        if !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
        }
    }

    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn clean_tag(value: String) -> Option<String> {
    let value = value.trim().trim_end_matches(',').trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn content_rating_from_tags(tags: Option<&Vec<String>>) -> ContentRating {
    let Some(tags) = tags else {
        return ContentRating::Safe;
    };
    let mut is_suggestive = false;

    for tag in tags {
        match tag.to_lowercase().as_str() {
            "erotique" | "smut" => return ContentRating::NSFW,
            "adulte" | "ecchi" | "gore" | "mature" | "violence" => {
                is_suggestive = true;
            }
            _ => {}
        }
    }

    if is_suggestive {
        ContentRating::Suggestive
    } else {
        ContentRating::Safe
    }
}

fn fetch_chapters(manga_key: &str, first_page_html: Document) -> Result<Vec<Chapter>> {
    let mut chapters = Vec::new();
    let mut page = 1;
    let mut html = first_page_html;

    loop {
        append_chapters(manga_key, &html, &mut chapters);

        if !has_next_page(&html, page) || page >= 25 {
            break;
        }

        page += 1;
        html = get_html(&manga_url(manga_key, page))?;
    }

    Ok(chapters)
}

fn append_chapters(manga_key: &str, html: &Document, chapters: &mut Vec<Chapter>) {
    let selector = chapter_link_selector(manga_key);
    let Some(links) = html.select(selector) else {
        return;
    };

    append_chapter_links(manga_key, links, chapters);
}

fn parse_chapter_links(manga_key: &str, element: &Element) -> Option<Vec<Chapter>> {
    let mut chapters = Vec::new();
    let links = element.select(chapter_link_selector(manga_key))?;
    append_chapter_links(manga_key, links, &mut chapters);

    if chapters.is_empty() {
        None
    } else {
        Some(chapters)
    }
}

fn append_chapter_links(manga_key: &str, links: ElementList, chapters: &mut Vec<Chapter>) {
    for link in links {
        let Some(url) = link.attr("abs:href") else {
            continue;
        };
        let Some(key) = chapter_key_from_url(manga_key, &url) else {
            continue;
        };

        if chapters.iter().any(|chapter| chapter.key == key) {
            continue;
        }

        chapters.push(Chapter {
            key: key.clone(),
            title: Some(format!("Chapitre {}", key)),
            chapter_number: key.parse::<f32>().ok(),
            url: Some(url),
            language: Some("fr".to_string()),
            ..Default::default()
        });
    }
}

fn chapter_link_selector(manga_key: &str) -> String {
    format!("a[href*='/manga/{}/']", manga_key)
}

fn has_next_page(html: &Document, page: i32) -> bool {
    let next_page = format!("page={}", page + 1);
    html.select_first(format!("li.pagination-link[onclick*='{}']", next_page))
        .is_some()
}

register_source!(LelscanFr, ListingProvider, ImageRequestProvider);
