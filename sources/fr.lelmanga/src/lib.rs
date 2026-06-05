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

const BASE_URL: &str = "https://www.lelmanga.com";
const USER_AGENT: &str = "Mozilla/5.0 (Aidoku)";
const CARD_SELECTOR: &str = ".listupd .bsx > a[href*='/manga/']";

struct Lelmanga;

impl Source for Lelmanga {
    fn new() -> Self {
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let url = search_url(query, page, &filters, None);
        let html = get_html(&url)?;

        Ok(MangaPageResult {
            entries: parse_manga_cards(html.select(CARD_SELECTOR)),
            has_next_page: has_next_page(&html),
        })
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let url = manga_url(&manga.key);
        let html = get_html(&url)?;

        if needs_details {
            if let Some(title) = html
                .select_first("h1.entry-title")
                .and_then(|element| element.text())
                .and_then(|value| non_empty_text(&value))
            {
                manga.title = title;
            }

            manga.cover = html
                .select_first(".thumb img")
                .and_then(|element| image_url(&element));
            manga.description = html
                .select_first(".info-desc .entry-content[itemprop='description']")
                .and_then(|element| element.text())
                .and_then(|value| non_empty_text(&value));
            manga.tags = parse_tags(&html);
            manga.status = manga_status(metadata_value(&html, "Status").as_deref());
            manga.url = Some(url.clone());
            manga.content_rating = content_rating_from_tags(manga.tags.as_ref());
            manga.viewer = viewer_from_metadata(
                metadata_value(&html, "Type").as_deref(),
                manga.tags.as_ref(),
            );
        }

        if needs_chapters {
            manga.chapters = Some(fetch_chapters(&html));
        }

        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let url = chapter
            .url
            .clone()
            .unwrap_or_else(|| format!("{}/{}", BASE_URL, chapter.key));
        let html = get_string(&url)?;
        let mut urls = parse_readerarea_images(&html);

        if urls.is_empty() {
            urls = parse_ts_reader_images(&html);
        }

        Ok(urls
            .into_iter()
            .map(|url| Page {
                content: PageContent::url(url),
                ..Default::default()
            })
            .collect())
    }
}

impl ListingProvider for Lelmanga {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        let order = match listing.id.as_str() {
            "popular" => Some("popular"),
            "new" => Some("latest"),
            "latest" => Some("update"),
            _ => None,
        };
        let url = search_url(None, page, &[], order);
        let html = get_html(&url)?;

        Ok(MangaPageResult {
            entries: parse_manga_cards(html.select(CARD_SELECTOR)),
            has_next_page: has_next_page(&html),
        })
    }
}

impl ImageRequestProvider for Lelmanga {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        Ok(Request::get(url)?
            .header("Referer", BASE_URL)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "image/*"))
    }
}

fn get_html(url: &str) -> Result<Document> {
    Ok(Request::get(url)?
        .header("User-Agent", USER_AGENT)
        .header("Referer", BASE_URL)
        .html()?)
}

fn get_string(url: &str) -> Result<String> {
    Ok(Request::get(url)?
        .header("User-Agent", USER_AGENT)
        .header("Referer", BASE_URL)
        .string()?)
}

fn parse_manga_cards(cards: Option<ElementList>) -> Vec<Manga> {
    let mut entries = Vec::new();

    if let Some(cards) = cards {
        for card in cards {
            let Some(url) = card.attr("abs:href") else {
                continue;
            };
            let Some(key) = manga_key_from_url(&url) else {
                continue;
            };

            let title = card
                .attr("title")
                .and_then(|value| non_empty_text(&decode_html_entities(&value)))
                .or_else(|| card.select_first(".tt").and_then(|element| element.text()))
                .and_then(|value| non_empty_text(&value))
                .or_else(|| {
                    card.select_first("img")
                        .and_then(|element| element.attr("alt"))
                        .and_then(|value| non_empty_text(&decode_html_entities(&value)))
                })
                .unwrap_or_else(|| key_to_title(&key));
            let cover = card
                .select_first("img")
                .and_then(|element| image_url(&element));
            let type_tag = type_from_card(&card);
            let tags = type_tag.as_ref().map(|value| vec![value.clone()]);
            let viewer = viewer_from_metadata(type_tag.as_deref(), tags.as_ref());

            push_unique_manga(
                &mut entries,
                Manga {
                    key,
                    title,
                    cover,
                    url: Some(url),
                    tags,
                    viewer,
                    content_rating: ContentRating::Safe,
                    ..Default::default()
                },
            );
        }
    }

    entries
}

fn push_unique_manga(entries: &mut Vec<Manga>, manga: Manga) {
    if !entries.iter().any(|entry| entry.key == manga.key) {
        entries.push(manga);
    }
}

fn search_url(
    query: Option<String>,
    page: i32,
    filters: &[FilterValue],
    forced_order: Option<&str>,
) -> String {
    let page = page.max(1);
    let mut params = Vec::new();

    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        push_query_param(&mut params, "s", &query);
        if page > 1 {
            push_query_param(&mut params, "page", &page.to_string());
        }
        return format!("{}?{}", BASE_URL, params.join("&"));
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

    if let Some(order) = forced_order {
        push_query_param(&mut params, "order", order);
    }

    if page > 1 {
        push_query_param(&mut params, "page", &page.to_string());
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
        "status" | "type" | "order" => push_query_param(params, id, value),
        _ => {}
    }
}

fn push_query_param(params: &mut Vec<String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        params.push(format!("{}={}", encode_uri(key), encode_uri(value)));
    }
}

fn has_next_page(html: &Document) -> bool {
    html.select_first("a.next.page-numbers").is_some()
        || html.select_first("link[rel='next']").is_some()
}

fn manga_url(key: &str) -> String {
    format!("{}/manga/{}", BASE_URL, key)
}

fn manga_key_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let marker = "/manga/";
    let start = path.find(marker)? + marker.len();
    let key = path[start..].split('/').next().unwrap_or_default();

    if key.is_empty() || key == "list-mode" {
        None
    } else {
        Some(key.to_string())
    }
}

fn chapter_key_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let key = path
        .trim_start_matches(BASE_URL)
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default();

    if key.is_empty() || key == "#!" || key == "#" || key.starts_with("manga") {
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

fn image_url(element: &Element) -> Option<String> {
    element
        .attr("abs:data-src")
        .or_else(|| element.attr("abs:src"))
        .or_else(|| element.attr("data-src"))
        .or_else(|| element.attr("src"))
        .map(|url| normalize_url(&url))
        .and_then(|url| {
            if is_valid_image_url(&url) {
                Some(url)
            } else {
                None
            }
        })
}

fn type_from_card(card: &Element) -> Option<String> {
    let class = card
        .select_first(".type")
        .and_then(|element| element.attr("class"))?;

    for part in class.split_whitespace() {
        match part.to_ascii_lowercase().as_str() {
            "manga" => return Some("Manga".to_string()),
            "manhwa" => return Some("Manhwa".to_string()),
            "manhua" => return Some("Manhua".to_string()),
            "comic" => return Some("Comic".to_string()),
            "novel" => return Some("Novel".to_string()),
            _ => {}
        }
    }

    None
}

fn parse_tags(html: &Document) -> Option<Vec<String>> {
    let mut tags = Vec::new();

    if let Some(elements) = html.select(".mgen a") {
        for element in elements {
            if let Some(tag) = element.text().and_then(|value| non_empty_text(&value)) {
                if !tags.iter().any(|entry| entry == &tag) {
                    tags.push(tag);
                }
            }
        }
    }

    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn metadata_value(html: &Document, label: &str) -> Option<String> {
    let label = label.to_ascii_lowercase();
    let elements = html.select(".imptdt")?;

    for element in elements {
        let Some(text) = element.text().and_then(|value| non_empty_text(&value)) else {
            continue;
        };
        let lower = text.to_ascii_lowercase();
        if lower.starts_with(&label) {
            return non_empty_text(text[label.len()..].trim());
        }
    }

    None
}

fn manga_status(status: Option<&str>) -> MangaStatus {
    match status.map(|value| value.to_ascii_lowercase()) {
        Some(value) if value.contains("ongoing") => MangaStatus::Ongoing,
        Some(value) if value.contains("completed") => MangaStatus::Completed,
        Some(value) if value.contains("hiatus") => MangaStatus::Hiatus,
        _ => MangaStatus::Unknown,
    }
}

fn viewer_from_metadata(type_value: Option<&str>, tags: Option<&Vec<String>>) -> Viewer {
    if type_value
        .map(|value| is_webtoonish(&value.to_ascii_lowercase()))
        .unwrap_or_default()
    {
        return Viewer::Webtoon;
    }

    if let Some(tags) = tags {
        if tags
            .iter()
            .any(|tag| is_webtoonish(&tag.to_ascii_lowercase()))
        {
            return Viewer::Webtoon;
        }
    }

    Viewer::RightToLeft
}

fn is_webtoonish(value: &str) -> bool {
    value.contains("manhwa")
        || value.contains("manhua")
        || value.contains("webtoon")
        || value.contains("webcomic")
}

fn content_rating_from_tags(tags: Option<&Vec<String>>) -> ContentRating {
    let Some(tags) = tags else {
        return ContentRating::Safe;
    };

    for tag in tags {
        let tag = tag.to_ascii_lowercase();
        if tag.contains("ecchi")
            || tag.contains("harem")
            || tag.contains("mature")
            || tag.contains("smut")
            || tag.contains("yaoi")
            || tag.contains("yuri")
        {
            return ContentRating::Suggestive;
        }
    }

    ContentRating::Safe
}

fn fetch_chapters(html: &Document) -> Vec<Chapter> {
    let mut chapters = Vec::new();

    if let Some(elements) = html.select("#chapterlist li") {
        for element in elements {
            let Some(link) = element.select_first("a[href]") else {
                continue;
            };
            let Some(url) = link.attr("abs:href") else {
                continue;
            };
            let Some(key) = chapter_key_from_url(&url) else {
                continue;
            };
            let label = link
                .select_first(".chapternum")
                .and_then(|element| element.text())
                .and_then(|value| non_empty_text(&value));
            let chapter_number = label.as_deref().and_then(parse_chapter_number);
            let title = label
                .as_deref()
                .and_then(|value| chapter_title(value, chapter_number));
            let date_uploaded = link
                .select_first(".chapterdate")
                .and_then(|element| element.text())
                .and_then(|value| parse_english_date(&value));

            chapters.push(Chapter {
                key,
                title,
                chapter_number,
                date_uploaded,
                url: Some(url),
                language: Some("fr".to_string()),
                ..Default::default()
            });
        }
    }

    chapters
}

fn parse_chapter_number(value: &str) -> Option<f32> {
    let mut number = String::new();
    let mut started = false;

    for char in value.chars() {
        if char.is_ascii_digit() {
            started = true;
            number.push(char);
        } else if started && (char == '.' || char == ',') {
            number.push('.');
        } else if started {
            break;
        }
    }

    number.parse::<f32>().ok()
}

fn chapter_title(label: &str, chapter_number: Option<f32>) -> Option<String> {
    let Some(number) = chapter_number else {
        return non_empty_text(label);
    };
    let mut label = label.trim();

    if let Some(rest) = label.strip_prefix("Chapter") {
        label = rest.trim();
    } else if let Some(rest) = label.strip_prefix("Chapitre") {
        label = rest.trim();
    }

    let number_text = trim_number_suffix(number);
    if let Some(rest) = label.strip_prefix(&number_text) {
        return non_empty_text(rest.trim());
    }

    None
}

fn trim_number_suffix(number: f32) -> String {
    let value = number.to_string();
    if let Some(rest) = value.strip_suffix(".0") {
        rest.to_string()
    } else {
        value
    }
}

fn parse_english_date(value: &str) -> Option<i64> {
    let cleaned = value.replace(',', "");
    let mut parts = cleaned.split_whitespace();
    let month = month_number(parts.next()?)?;
    let day = parts.next()?.parse::<i64>().ok()?;
    let year = parts.next()?.parse::<i64>().ok()?;
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400)
}

fn month_number(value: &str) -> Option<i64> {
    match value {
        "January" => Some(1),
        "February" => Some(2),
        "March" => Some(3),
        "April" => Some(4),
        "May" => Some(5),
        "June" => Some(6),
        "July" => Some(7),
        "August" => Some(8),
        "September" => Some(9),
        "October" => Some(10),
        "November" => Some(11),
        "December" => Some(12),
        _ => None,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_adjusted = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * month_adjusted + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn parse_readerarea_images(html: &str) -> Vec<String> {
    let Some(start) = html.find("id=\"readerarea\"") else {
        return Vec::new();
    };
    let segment = &html[start..];
    let end = segment.find("</noscript>").unwrap_or(segment.len());
    let segment = &segment[..end];
    let mut urls = extract_attr_urls(segment, "src=\"");
    urls.extend(extract_attr_urls(segment, "data-src=\""));
    dedupe_urls(urls)
}

fn extract_attr_urls(segment: &str, marker: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = segment;

    while let Some(start) = rest.find(marker) {
        rest = &rest[start + marker.len()..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let url = normalize_url(&decode_html_entities(&rest[..end]));
        if is_valid_image_url(&url) {
            urls.push(url);
        }
        rest = &rest[end..];
    }

    urls
}

fn parse_ts_reader_images(html: &str) -> Vec<String> {
    let Some(start) = html.find("\"images\":[") else {
        return Vec::new();
    };
    let rest = &html[start + "\"images\":[".len()..];
    let Some(end) = rest.find(']') else {
        return Vec::new();
    };

    dedupe_urls(
        parse_quoted_strings(&rest[..end])
            .into_iter()
            .map(|url| normalize_url(&url))
            .filter(|url| is_valid_image_url(url))
            .collect(),
    )
}

fn parse_quoted_strings(value: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = value.chars();

    while let Some(char) = chars.next() {
        if !in_string {
            if char == '"' {
                in_string = true;
                current.clear();
            }
            continue;
        }

        if char == '\\' {
            let Some(next) = chars.next() else {
                break;
            };
            match next {
                '/' => current.push('/'),
                '"' => current.push('"'),
                '\\' => current.push('\\'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(hex_char) = chars.next() {
                            hex.push(hex_char);
                        }
                    }
                    if hex == "0026" {
                        current.push('&');
                    }
                }
                other => current.push(other),
            }
        } else if char == '"' {
            in_string = false;
            strings.push(current.clone());
        } else {
            current.push(char);
        }
    }

    strings
}

fn dedupe_urls(urls: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();

    for url in urls {
        if !deduped.iter().any(|entry| entry == &url) {
            deduped.push(url);
        }
    }

    deduped
}

fn is_valid_image_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && !lower.contains("readerarea.svg")
        && !lower.contains("btn_close.gif")
        && !lower.contains("/themes/")
}

fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{}", url)
    } else {
        url.to_string()
    }
}

fn non_empty_text(value: &str) -> Option<String> {
    let value = decode_html_entities(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#038;", "&")
        .replace("&quot;", "\"")
        .replace("&#8217;", "'")
        .replace("&#8211;", "-")
        .replace("&#8212;", "-")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

register_source!(Lelmanga, ListingProvider, ImageRequestProvider);
