//! Render prose without executing document HTML or loading remote images.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::collections::BTreeMap;

fn body(markdown: &str) -> &str {
    let text = markdown.trim_start_matches('\u{feff}');
    if let Some(after) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        let mut offset = 0;
        for line in after.split_inclusive('\n') {
            if line.trim_end_matches(['\n', '\r']) == "---" {
                return &after[offset + line.len()..];
            }
            offset += line.len();
        }
    }
    text
}

fn destination(base: &str, href: &str) -> String {
    if href.starts_with('#')
        || href.starts_with("https://")
        || href.starts_with("http://")
        || href.starts_with("mailto:")
    {
        return href.into();
    }
    if href.contains(':') || href.starts_with('/') || href.contains('\\') {
        return String::new();
    }
    let (path, anchor) = href.split_once('#').unwrap_or((href, ""));
    if !path.ends_with(".md") {
        return String::new();
    }
    let mut parts: Vec<_> = base.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return String::new();
                }
            }
            _ => parts.push(part),
        }
    }
    format!(
        "#knowledge:{}{}",
        urlencoding::encode(&parts.join("/")),
        if anchor.is_empty() {
            String::new()
        } else {
            format!("#{}", urlencoding::encode(anchor))
        }
    )
}

pub(super) fn render(markdown: &str, path: &str) -> String {
    let mut events: Vec<_> = Parser::new_ext(
        body(markdown),
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .collect();
    let mut headings = BTreeMap::<String, usize>::new();
    for index in 0..events.len() {
        if matches!(events[index], Event::Start(Tag::Heading { .. })) {
            let title: String = events[index + 1..]
                .iter()
                .take_while(|e| !matches!(e, Event::End(TagEnd::Heading(_))))
                .filter_map(|event| match event {
                    Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                    _ => None,
                })
                .collect();
            let slug: String = title
                .to_lowercase()
                .chars()
                .filter_map(|ch| {
                    if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                        Some(ch)
                    } else if ch.is_whitespace() {
                        Some('-')
                    } else {
                        None
                    }
                })
                .collect();
            let count = headings.entry(slug.clone()).or_default();
            let slug = if *count == 0 {
                slug
            } else {
                format!("{slug}-{count}")
            };
            *count += 1;
            if let Event::Start(Tag::Heading { id, .. }) = &mut events[index] {
                *id = Some(slug.into());
            }
        }
    }
    let safe = events.into_iter().map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        })
        | Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: destination(path, &dest_url).into(),
            title,
            id,
        }),
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        event => event,
    });
    let mut output = String::new();
    html::push_html(&mut output, safe);
    // External references open independently of the editor's navigation scope.
    output.replace(
        "<a href=\"http",
        "<a target=\"_blank\" rel=\"noopener noreferrer\" href=\"http",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    #[test]
    fn preview_escapes_html_and_preserves_safe_document_navigation() {
        let html = render(
            "---\nid: document\n---\n# Design\n\n<script>alert(1)</script>\n\n[x](javascript:alert) ![remote](https://example.org/image.png) [parent](../README.md#overview)",
            "domain/detail.md",
        );
        assert_that!(&html).does_not_contain("<script>");
        assert_that!(&html).does_not_contain("javascript:");
        assert_that!(&html).does_not_contain("<img");
        assert_that!(&html).does_not_contain("id: document");
        assert_that!(&html).contains("#knowledge:README.md#overview");
        assert_that!(&html).contains("id=\"design\"");
    }
}
