#![no_std]

extern crate alloc;

use aidoku::{
    helpers::uri::encode_uri,
    imports::net::{set_rate_limit, Request, TimeUnit},
    prelude::*,
    Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
    MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use serde::{de::DeserializeOwned, Deserialize};

const API_URL: &str = "https://api.mangadex.org";
const SITE_URL: &str = "https://mangadex.org";
const UPLOADS_URL: &str = "https://uploads.mangadex.org";
const USER_AGENT: &str = "Aidoku-Sources/1.0 (https://github.com/K0Konut/aidoku-Sources)";
const PAGE_SIZE: i32 = 20;
const CHAPTER_PAGE_SIZE: i32 = 500;
const DEFAULT_CONTENT_RATINGS: [&str; 3] = ["safe", "suggestive", "erotica"];

struct MangaDexFr;

impl Source for MangaDexFr {
    fn new() -> Self {
        set_rate_limit(4, 1, TimeUnit::Seconds);
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        fetch_manga_list(query, page, &filters, SearchOrder::LatestUploadedChapter)
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        if needs_details {
            let mut params = Vec::new();
            push_default_manga_includes(&mut params);
            let path = api_path(&format!("/manga/{}", manga.key), params);
            let response: ApiEntity<MangaEntity> = api_get(path)?;
            let updated = manga_from_entity(response.data);
            manga.copy_from(updated);
        }

        if needs_chapters {
            manga.chapters = Some(fetch_chapters(&manga.key)?);
        }

        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let path = format!("/at-home/server/{}", chapter.key);
        let response: AtHomeResponse = api_get(path)?;
        let (quality, files) = if response.chapter.data.is_empty() {
            ("data-saver", response.chapter.data_saver)
        } else {
            ("data", response.chapter.data)
        };

        Ok(files
            .into_iter()
            .map(|file| Page {
                content: PageContent::url(format!(
                    "{}/{}/{}/{}",
                    response.base_url, quality, response.chapter.hash, file
                )),
                ..Default::default()
            })
            .collect())
    }
}

impl ListingProvider for MangaDexFr {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        let order = match listing.id.as_str() {
            "popular" => SearchOrder::FollowedCount,
            "new" => SearchOrder::CreatedAt,
            _ => SearchOrder::LatestUploadedChapter,
        };
        fetch_manga_list(None, page, &[], order)
    }
}

impl ImageRequestProvider for MangaDexFr {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        Ok(Request::get(url)?
            .header("User-Agent", USER_AGENT)
            .header("Accept", "image/*"))
    }
}

#[derive(Clone, Copy)]
enum SearchOrder {
    LatestUploadedChapter,
    FollowedCount,
    CreatedAt,
}

#[derive(Deserialize)]
struct ApiCollection<T> {
    data: Vec<T>,
    #[serde(default)]
    limit: i32,
    #[serde(default)]
    offset: i32,
    #[serde(default)]
    total: i32,
}

#[derive(Deserialize)]
struct ApiEntity<T> {
    data: T,
}

#[derive(Deserialize)]
struct Entity<T> {
    id: String,
    attributes: T,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Deserialize)]
struct Relationship {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attributes: Option<RelationshipAttributes>,
}

#[derive(Default, Deserialize)]
struct RelationshipAttributes {
    #[serde(rename = "fileName")]
    file_name: Option<String>,
    name: Option<String>,
}

#[derive(Default, Deserialize)]
struct MangaAttributes {
    #[serde(default)]
    title: aidoku::HashMap<String, String>,
    #[serde(default)]
    description: aidoku::HashMap<String, String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "contentRating", default)]
    content_rating: Option<String>,
    #[serde(default)]
    tags: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    attributes: TagAttributes,
}

#[derive(Default, Deserialize)]
struct TagAttributes {
    #[serde(default)]
    name: aidoku::HashMap<String, String>,
}

#[derive(Default, Deserialize)]
struct ChapterAttributes {
    volume: Option<String>,
    chapter: Option<String>,
    title: Option<String>,
    #[serde(rename = "translatedLanguage")]
    translated_language: Option<String>,
    #[serde(rename = "externalUrl")]
    external_url: Option<String>,
    pages: Option<i32>,
    #[serde(rename = "publishAt")]
    publish_at: Option<String>,
}

#[derive(Deserialize)]
struct AtHomeResponse {
    #[serde(rename = "baseUrl")]
    base_url: String,
    chapter: AtHomeChapter,
}

#[derive(Deserialize)]
struct AtHomeChapter {
    hash: String,
    #[serde(default)]
    data: Vec<String>,
    #[serde(rename = "dataSaver", default)]
    data_saver: Vec<String>,
}

type MangaEntity = Entity<MangaAttributes>;
type ChapterEntity = Entity<ChapterAttributes>;

fn api_get<T>(path: String) -> Result<T>
where
    T: DeserializeOwned,
{
    Ok(Request::get(format!("{}{}", API_URL, path))?
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .json_owned()?)
}

fn fetch_manga_list(
    query: Option<String>,
    page: i32,
    filters: &[FilterValue],
    order: SearchOrder,
) -> Result<MangaPageResult> {
    let path = manga_list_path(query, page, filters, order);
    let response: ApiCollection<MangaEntity> = api_get(path)?;
    let next_offset = response.offset + response.limit.max(response.data.len() as i32);
    let has_next_page = next_offset < response.total;

    Ok(MangaPageResult {
        entries: response.data.into_iter().map(manga_from_entity).collect(),
        has_next_page,
    })
}

fn manga_list_path(
    query: Option<String>,
    page: i32,
    filters: &[FilterValue],
    order: SearchOrder,
) -> String {
    let mut params = Vec::new();
    let offset = (page.max(1) - 1) * PAGE_SIZE;

    push_param(&mut params, "limit", &PAGE_SIZE.to_string());
    push_param(&mut params, "offset", &offset.to_string());
    push_param(&mut params, "availableTranslatedLanguage[]", "fr");
    push_param(&mut params, "hasAvailableChapters", "true");

    push_default_manga_includes(&mut params);

    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        push_param(&mut params, "title", query.trim());
    }

    let mut has_content_rating_filter = false;
    for filter in filters {
        match filter {
            FilterValue::MultiSelect { id, included, .. } if id == "status" => {
                for value in included {
                    push_param(&mut params, "status[]", value);
                }
            }
            FilterValue::MultiSelect { id, included, .. } if id == "demographic" => {
                for value in included {
                    push_param(&mut params, "publicationDemographic[]", value);
                }
            }
            FilterValue::MultiSelect { id, included, .. } if id == "rating" => {
                has_content_rating_filter = !included.is_empty();
                for value in included {
                    push_param(&mut params, "contentRating[]", value);
                }
            }
            _ => {}
        }
    }

    if !has_content_rating_filter {
        push_default_content_ratings(&mut params);
    }

    match order {
        SearchOrder::LatestUploadedChapter => {
            push_param(&mut params, "order[latestUploadedChapter]", "desc")
        }
        SearchOrder::FollowedCount => push_param(&mut params, "order[followedCount]", "desc"),
        SearchOrder::CreatedAt => push_param(&mut params, "order[createdAt]", "desc"),
    }

    api_path("/manga", params)
}

fn fetch_chapters(manga_id: &str) -> Result<Vec<Chapter>> {
    let mut chapters = Vec::new();
    let mut offset = 0;

    loop {
        let mut params = Vec::new();
        push_param(&mut params, "limit", &CHAPTER_PAGE_SIZE.to_string());
        push_param(&mut params, "offset", &offset.to_string());
        push_param(&mut params, "translatedLanguage[]", "fr");
        push_param(&mut params, "includeFutureUpdates", "0");
        push_param(&mut params, "includeEmptyPages", "0");
        push_param(&mut params, "includes[]", "scanlation_group");
        push_default_content_ratings(&mut params);
        push_param(&mut params, "order[volume]", "desc");
        push_param(&mut params, "order[chapter]", "desc");

        let path = api_path(&format!("/manga/{}/feed", manga_id), params);
        let response: ApiCollection<ChapterEntity> = api_get(path)?;
        let count = response.data.len() as i32;

        for chapter in response.data {
            if let Some(chapter) = chapter_from_entity(chapter) {
                chapters.push(chapter);
            }
        }

        if count == 0 {
            break;
        }

        let next_offset = response.offset + response.limit.max(count);
        if count < CHAPTER_PAGE_SIZE || next_offset >= response.total {
            break;
        }
        offset = next_offset;
    }

    Ok(chapters)
}

fn chapter_from_entity(entity: ChapterEntity) -> Option<Chapter> {
    let attributes = entity.attributes;
    if attributes.external_url.is_some() || attributes.pages.unwrap_or_default() <= 0 {
        return None;
    }

    Some(Chapter {
        key: entity.id.clone(),
        title: chapter_title(&attributes),
        chapter_number: attributes.chapter.as_deref().and_then(parse_number),
        volume_number: attributes.volume.as_deref().and_then(parse_number),
        date_uploaded: attributes
            .publish_at
            .as_deref()
            .and_then(parse_rfc3339_timestamp),
        scanlators: relationship_names(&entity.relationships, "scanlation_group"),
        url: Some(format!("{}/chapter/{}", SITE_URL, entity.id)),
        language: attributes.translated_language,
        ..Default::default()
    })
}

fn manga_from_entity(entity: MangaEntity) -> Manga {
    let title = localized_value(&entity.attributes.title).unwrap_or_else(|| entity.id.clone());
    let description = localized_value(&entity.attributes.description);
    let cover = cover_url(&entity);
    let authors = relationship_names(&entity.relationships, "author");
    let artists = relationship_names(&entity.relationships, "artist");
    let tags = tags_from_attributes(&entity.attributes);
    let status = manga_status(entity.attributes.status.as_deref());
    let content_rating = content_rating(entity.attributes.content_rating.as_deref());

    Manga {
        key: entity.id.clone(),
        title,
        cover,
        authors,
        artists,
        description,
        url: Some(format!("{}/title/{}", SITE_URL, entity.id)),
        tags,
        status,
        content_rating,
        viewer: Viewer::RightToLeft,
        ..Default::default()
    }
}

fn push_default_manga_includes(params: &mut Vec<String>) {
    push_param(params, "includes[]", "cover_art");
    push_param(params, "includes[]", "author");
    push_param(params, "includes[]", "artist");
}

fn push_default_content_ratings(params: &mut Vec<String>) {
    for rating in DEFAULT_CONTENT_RATINGS {
        push_param(params, "contentRating[]", rating);
    }
}

fn api_path(path: &str, params: Vec<String>) -> String {
    if params.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, params.join("&"))
    }
}

fn push_param(params: &mut Vec<String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        params.push(format!("{}={}", encode_uri(key), encode_uri(value)));
    }
}

fn localized_value(values: &aidoku::HashMap<String, String>) -> Option<String> {
    for language in ["fr", "en", "ja-ro", "ja"] {
        if let Some(value) = non_empty(values.get(language)) {
            return Some(value);
        }
    }

    values.iter().find_map(|(_, value)| non_empty(Some(value)))
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn cover_url(entity: &MangaEntity) -> Option<String> {
    entity.relationships.iter().find_map(|relationship| {
        if relationship.kind != "cover_art" {
            return None;
        }

        relationship
            .attributes
            .as_ref()
            .and_then(|attributes| attributes.file_name.as_ref())
            .map(|file_name| format!("{}/covers/{}/{}.512.jpg", UPLOADS_URL, entity.id, file_name))
    })
}

fn relationship_names(relationships: &[Relationship], kind: &str) -> Option<Vec<String>> {
    let names = relationships
        .iter()
        .filter(|relationship| relationship.kind == kind)
        .filter_map(|relationship| {
            relationship
                .attributes
                .as_ref()
                .and_then(|attributes| attributes.name.as_ref())
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();

    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

fn tags_from_attributes(attributes: &MangaAttributes) -> Option<Vec<String>> {
    let tags = attributes
        .tags
        .iter()
        .filter_map(|tag| localized_value(&tag.attributes.name))
        .collect::<Vec<_>>();

    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn manga_status(status: Option<&str>) -> MangaStatus {
    match status {
        Some("ongoing") => MangaStatus::Ongoing,
        Some("completed") => MangaStatus::Completed,
        Some("cancelled") => MangaStatus::Cancelled,
        Some("hiatus") => MangaStatus::Hiatus,
        _ => MangaStatus::Unknown,
    }
}

fn content_rating(rating: Option<&str>) -> ContentRating {
    match rating {
        Some("safe") => ContentRating::Safe,
        Some("suggestive") => ContentRating::Suggestive,
        Some("erotica") | Some("pornographic") => ContentRating::NSFW,
        _ => ContentRating::Unknown,
    }
}

fn chapter_title(attributes: &ChapterAttributes) -> Option<String> {
    if let Some(title) = attributes
        .title
        .as_ref()
        .map(|title| title.trim())
        .filter(|title| !title.is_empty())
    {
        return Some(title.to_string());
    }

    if attributes.chapter.is_none() {
        Some("One-shot".to_string())
    } else {
        None
    }
}

fn parse_number(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok()
}

fn parse_rfc3339_timestamp(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return None;
    }

    let year = parse_i64(&bytes[0..4])?;
    let month = parse_i64(&bytes[5..7])?;
    let day = parse_i64(&bytes[8..10])?;
    let hour = parse_i64(&bytes[11..13])?;
    let minute = parse_i64(&bytes[14..16])?;
    let second = parse_i64(&bytes[17..19])?;
    let days = days_from_civil(year, month, day)?;
    let timestamp = days * 86_400 + hour * 3_600 + minute * 60 + second;

    match bytes.get(19).copied() {
        Some(b'Z') => Some(timestamp),
        Some(b'+') | Some(b'-') if bytes.len() >= 25 => {
            let sign = if bytes[19] == b'+' { 1_i64 } else { -1_i64 };
            let offset_hour = parse_i64(&bytes[20..22])?;
            let offset_minute = parse_i64(&bytes[23..25])?;
            Some(timestamp - sign * (offset_hour * 3_600 + offset_minute * 60))
        }
        _ => None,
    }
}

fn parse_i64(bytes: &[u8]) -> Option<i64> {
    let mut value = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(*byte - b'0');
    }
    Some(value)
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

register_source!(MangaDexFr, ListingProvider, ImageRequestProvider);
