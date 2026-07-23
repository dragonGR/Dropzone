// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

/// Escapes characters with special meaning in HTML to prevent XSS.
pub fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            _ => output.push(c),
        }
    }
    output
}

/// Formats a Content-Disposition header safely according to RFC 6266 and RFC 5987.
///
/// Prevents HTTP header injection (CRLF), directory traversal, and safely supports
/// arbitrary Unicode filenames via `filename*=UTF-8''...` parameter.
pub fn format_content_disposition(raw_filename: &str) -> String {
    let sanitized: String = raw_filename
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .collect();

    let base_name = Path::new(&sanitized)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");

    let ascii_fallback: String = base_name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' && c != ';' {
                c
            } else if c == ' ' {
                ' '
            } else {
                '_'
            }
        })
        .collect();

    let mut rfc5987_encoded = String::from("UTF-8''");
    for b in base_name.as_bytes() {
        match *b {
            b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~' => {
                rfc5987_encoded.push(*b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(rfc5987_encoded, "%{:02X}", *b);
            }
        }
    }

    format!(
        "attachment; filename=\"{}\"; filename*={}",
        ascii_fallback, rfc5987_encoded
    )
}

/// Informs the receiver browser of the media type based on standard file extensions.
pub fn guess_mime_type(filename: &str) -> &'static str {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("tar") => "application/x-tar",
        Some("gz") => "application/gzip",
        Some("mp4") => "video/mp4",
        Some("mp3") => "audio/mpeg",
        Some("txt") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("Hello World"), "Hello World");
        assert_eq!(
            escape_html("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
        assert_eq!(
            escape_html("A & B \"quotes\""),
            "A &amp; B &quot;quotes&quot;"
        );
    }

    #[test]
    fn test_content_disposition_normal() {
        let cd = format_content_disposition("document.pdf");
        assert_eq!(
            cd,
            "attachment; filename=\"document.pdf\"; filename*=UTF-8''document.pdf"
        );
    }

    #[test]
    fn test_content_disposition_crlf_injection_neutralized() {
        let cd = format_content_disposition("test\r\nInjected-Header: bad\r\n.txt");
        assert!(!cd.contains('\r'));
        assert!(!cd.contains('\n'));
        assert!(cd.starts_with("attachment; filename="));
    }

    #[test]
    fn test_content_disposition_path_traversal_neutralized() {
        let cd = format_content_disposition("../../etc/passwd");
        assert_eq!(
            cd,
            "attachment; filename=\"passwd\"; filename*=UTF-8''passwd"
        );
    }

    #[test]
    fn test_content_disposition_unicode_and_spaces() {
        let cd = format_content_disposition("résumé photo.png");
        assert!(cd.contains("filename=\"r_sum_ photo.png\""));
        assert!(cd.contains("filename*=UTF-8''r%C3%A9sum%C3%A9%20photo.png"));
    }

    #[test]
    fn test_content_disposition_quotes_and_backslashes() {
        let cd = format_content_disposition("my \"cool\" file\\test.zip");
        assert!(!cd.contains("\"cool\""));
    }

    #[test]
    fn test_guess_mime_type() {
        assert_eq!(guess_mime_type("photo.png"), "image/png");
        assert_eq!(guess_mime_type("photo.JPG"), "image/jpeg");
        assert_eq!(guess_mime_type("doc.pdf"), "application/pdf");
        assert_eq!(guess_mime_type("archive.zip"), "application/zip");
        assert_eq!(guess_mime_type("notes.txt"), "text/plain; charset=utf-8");
        assert_eq!(guess_mime_type("unknown.xyz"), "application/octet-stream");
        assert_eq!(guess_mime_type("noextension"), "application/octet-stream");
    }
}
