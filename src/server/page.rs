// SPDX-License-Identifier: GPL-3.0-or-later

use crate::server::download::escape_html;
use crate::share::files::SharedFile;
use gettextrs::gettext;

const INDEX_HTML_TEMPLATE: &str = include_str!("../../web/index.html");

/// The receiver page's interface text, all in one language.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PageText {
    lang: String,
    region_label: String,
    size_label: String,
    download_label: String,
}

/// Renders the receiver page for `file`, reachable under `token`.
pub fn render_landing_page(file: &SharedFile, token: &str) -> String {
    let text = translated_page_text();
    let download_url = format!("/s/{}/files/{}", token, file.id().as_str());
    render_template(
        INDEX_HTML_TEMPLATE,
        &[
            ("LANG", &escape_html(&text.lang)),
            ("REGION_LABEL", &escape_html(&text.region_label)),
            ("SIZE_LABEL", &escape_html(&text.size_label)),
            ("DOWNLOAD_LABEL", &escape_html(&text.download_label)),
            ("FILE_NAME", &escape_html(file.name())),
            ("FILE_SIZE", &escape_html(&file.formatted_size())),
            ("DOWNLOAD_URL", &escape_html(&download_url)),
            ("TOKEN", &escape_html(token)),
        ],
    )
}

fn translated_page_text() -> PageText {
    choose_page_text(
        [
            // Translators: accessible name of the web page that receiving devices open
            ("Shared file", gettext("Shared file")),
            // Translators: accessible name of the file size on the receiving device's web page
            ("File size", gettext("File size")),
            // Translators: button on the web page that receiving devices open
            ("Download", gettext("Download")),
        ],
        &gettext(""),
    )
}

/// Uses the catalog's language only when every page string is translated, so the
/// page never mixes languages or declares a language it is not written in.
fn choose_page_text(texts: [(&str, String); 3], catalog_header: &str) -> PageText {
    let fully_translated = texts
        .iter()
        .all(|(msgid, translated)| translated.as_str() != *msgid);
    let language = language_tag(catalog_header).filter(|_| fully_translated);
    let [region, size, download] = texts;
    match language {
        Some(lang) => PageText {
            lang,
            region_label: region.1,
            size_label: size.1,
            download_label: download.1,
        },
        None => PageText {
            lang: "en".to_string(),
            region_label: region.0.to_string(),
            size_label: size.0.to_string(),
            download_label: download.0.to_string(),
        },
    }
}

/// Reads the language from a gettext catalog header (`gettext("")`) and returns it
/// as a BCP 47 tag, such as `pt-BR` for `pt_BR`.
fn language_tag(catalog_header: &str) -> Option<String> {
    let value = catalog_header
        .lines()
        .find_map(|line| line.strip_prefix("Language:"))?
        .trim();
    let locale = value.split(['.', '@']).next()?;
    let mut subtags = locale.split(['_', '-']);

    let primary = subtags.next()?;
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut tag = primary.to_ascii_lowercase();
    for subtag in subtags {
        if subtag.is_empty()
            || subtag.len() > 8
            || !subtag.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return None;
        }
        tag.push('-');
        tag.push_str(subtag);
    }
    Some(tag)
}

/// Replaces each `{{KEY}}` in `template` with its value in one pass, so a value
/// that happens to contain a placeholder is never expanded. Values must already be
/// escaped for HTML.
fn render_template(template: &str, values: &[(&str, &str)]) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            rest = &rest[start..];
            break;
        };
        let key = &after_open[..end];
        match values.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => rendered.push_str(value),
            None => rendered.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after_open[end + 2..];
    }
    rendered.push_str(rest);
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(region: &str, size: &str, download: &str) -> [(&'static str, String); 3] {
        [
            ("Shared file", region.to_string()),
            ("File size", size.to_string()),
            ("Download", download.to_string()),
        ]
    }

    #[test]
    fn test_template_values_are_not_expanded_again() {
        let rendered = render_template(
            "<p>{{NAME}}</p><a href=\"{{URL}}\">",
            &[("NAME", "{{URL}}"), ("URL", "/s/abc")],
        );
        assert_eq!(rendered, "<p>{{URL}}</p><a href=\"/s/abc\">");
    }

    #[test]
    fn test_template_keeps_unknown_and_unterminated_placeholders() {
        assert_eq!(
            render_template("{{A}}{{B}} {{C", &[("A", "1")]),
            "1{{B}} {{C"
        );
    }

    #[test]
    fn test_language_tag_is_read_from_catalog_header() {
        let header = "Project-Id-Version: dropzone\nLanguage: el\nMIME-Version: 1.0\n";
        assert_eq!(language_tag(header).as_deref(), Some("el"));
        assert_eq!(language_tag("Language: pt_BR\n").as_deref(), Some("pt-BR"));
        assert_eq!(language_tag("Language: sr@latin\n").as_deref(), Some("sr"));
        assert_eq!(
            language_tag("Language: el_GR.UTF-8\n").as_deref(),
            Some("el-GR")
        );
    }

    #[test]
    fn test_invalid_or_missing_language_is_rejected() {
        assert_eq!(language_tag(""), None);
        assert_eq!(language_tag("Project-Id-Version: dropzone\n"), None);
        assert_eq!(language_tag("Language: \n"), None);
        assert_eq!(language_tag("Language: \"><script>\n"), None);
        assert_eq!(language_tag("Language: e\n"), None);
    }

    #[test]
    fn test_fully_translated_page_uses_catalog_language() {
        let text = choose_page_text(
            texts("Κοινόχρηστο αρχείο", "Μέγεθος αρχείου", "Λήψη"),
            "Language: el\n",
        );
        assert_eq!(text.lang, "el");
        assert_eq!(text.download_label, "Λήψη");
        assert_eq!(text.region_label, "Κοινόχρηστο αρχείο");
    }

    #[test]
    fn test_partially_translated_page_stays_english() {
        let text = choose_page_text(
            texts("Κοινόχρηστο αρχείο", "File size", "Λήψη"),
            "Language: el\n",
        );
        assert_eq!(text.lang, "en");
        assert_eq!(text.download_label, "Download");
        assert_eq!(text.region_label, "Shared file");
    }

    #[test]
    fn test_untranslated_or_unlabelled_catalog_gives_english() {
        let untranslated = choose_page_text(texts("Shared file", "File size", "Download"), "");
        assert_eq!(untranslated.lang, "en");
        let unlabelled = choose_page_text(
            texts("Κοινόχρηστο αρχείο", "Μέγεθος αρχείου", "Λήψη"),
            "Project-Id-Version: dropzone\n",
        );
        assert_eq!(unlabelled.lang, "en");
        assert_eq!(unlabelled.download_label, "Download");
    }
}
