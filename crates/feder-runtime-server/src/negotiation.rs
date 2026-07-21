// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use axum::http::{HeaderMap, header::ACCEPT};
use mime::Mime;

const ACTIVITYPUB_MEDIA_TYPES: &[&str] = &[
    "application/activity+json",
    "application/ld+json",
    "application/json",
];
const HTML_MEDIA_TYPES: &[&str] = &["text/html", "application/xhtml+xml"];

struct MediaRange {
    media_type: Mime,
    quality: u16,
    order: usize,
}

#[derive(Clone, Copy)]
struct Preference {
    quality: u16,
    order: usize,
}

pub(crate) fn accepts_activitypub(headers: &HeaderMap) -> bool {
    let ranges = headers
        .get_all(ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| value.trim().parse::<Mime>().ok())
        .enumerate()
        .filter_map(|(order, media_range)| {
            let quality = media_range
                .get_param("q")
                .map_or(Some(1000), |value| parse_quality(value.as_str()))?;
            (quality > 0).then_some(MediaRange {
                media_type: media_range,
                quality,
                order,
            })
        })
        .collect::<Vec<_>>();

    let activitypub = preferred(ACTIVITYPUB_MEDIA_TYPES, &ranges);
    let html = preferred(HTML_MEDIA_TYPES, &ranges);

    match (activitypub, html) {
        (Some(activitypub), Some(html)) => prefers(activitypub, html),
        (Some(_), None) => true,
        _ => false,
    }
}

fn preferred(media_types: &[&str], ranges: &[MediaRange]) -> Option<Preference> {
    ranges
        .iter()
        .filter(|range| media_types.contains(&range.media_type.essence_str()))
        .map(|range| Preference {
            quality: range.quality,
            order: range.order,
        })
        .reduce(|current, candidate| {
            if prefers(candidate, current) {
                candidate
            } else {
                current
            }
        })
}

fn prefers(left: Preference, right: Preference) -> bool {
    left.quality > right.quality || (left.quality == right.quality && left.order < right.order)
}

fn parse_quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    match whole {
        "0" => {
            let padding = 3 - fraction.len();
            let fraction = fraction.parse::<u16>().unwrap_or(0);
            Some(fraction * 10_u16.pow(u32::try_from(padding).ok()?))
        }
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(1000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(accept: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(accept) = accept {
            headers.insert(ACCEPT, HeaderValue::from_str(accept).expect("valid header"));
        }
        headers
    }

    #[test]
    fn accepts_explicit_activitypub_media_types() {
        for accept in [
            Some("application/activity+json"),
            Some("application/ld+json"),
            Some("application/json"),
        ] {
            assert!(accepts_activitypub(&headers(accept)), "Accept: {accept:?}");
        }
    }

    #[test]
    fn rejects_implicit_html_or_unsupported_media_types() {
        for accept in [
            "",
            "*/*",
            "application/*",
            "text/html",
            "application/xhtml+xml",
            "image/png",
            "application/activity+json;q=0",
        ] {
            assert!(
                !accepts_activitypub(&headers(Some(accept))),
                "Accept: {accept}"
            );
        }
        assert!(!accepts_activitypub(&headers(None)));
    }

    #[test]
    fn respects_quality_and_order() {
        assert_eq!(parse_quality("0.9"), Some(900));
        assert_eq!(parse_quality("0.08"), Some(80));
        assert_eq!(parse_quality("1.000"), Some(1000));
        assert!(!accepts_activitypub(&headers(Some(
            "application/activity+json;q=0.5, text/html;q=0.8"
        ))));
        assert!(accepts_activitypub(&headers(Some(
            "application/activity+json;q=0.9, text/html;q=0.8"
        ))));
        assert!(!accepts_activitypub(&headers(Some(
            "text/html, application/activity+json"
        ))));
        assert!(accepts_activitypub(&headers(Some(
            "application/activity+json, text/html"
        ))));
        assert!(!accepts_activitypub(&headers(Some("text/html, */*"))));
        assert!(!accepts_activitypub(&headers(Some(
            "application/activity+json;q=0, application/ld+json;q=0, application/json;q=0, */*;q=1"
        ))));
    }

    #[test]
    fn ignores_unsupported_types_when_comparing_supported_representations() {
        assert!(!accepts_activitypub(&headers(Some(
            "image/png, application/activity+json;q=0.5, text/html;q=0.8"
        ))));
        assert!(accepts_activitypub(&headers(Some(
            "image/png, application/activity+json;q=0.8, text/html;q=0.5"
        ))));
        assert!(accepts_activitypub(&headers(Some(
            "image/png, application/activity+json;q=0.5"
        ))));
    }

    #[test]
    fn compares_the_best_type_in_each_supported_representation() {
        assert!(!accepts_activitypub(&headers(Some(
            "text/html;q=0.2, application/xhtml+xml;q=0.9, application/activity+json;q=0.8"
        ))));
        assert!(accepts_activitypub(&headers(Some(
            "text/html;q=0.8, application/activity+json;q=0.2, application/ld+json;q=0.9"
        ))));
    }
}
