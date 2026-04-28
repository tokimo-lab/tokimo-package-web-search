//! Google Web — 1:1 参考 searxng `engines/google.py`
//!
//! - URL: https://www.google.com/search?q=...&start=...
//! - Cookie: CONSENT=YES+
//! - 解析 `a[data-ved]:not([class])`，title 在 `div[style]`、content 在
//!   `div[class~="ilUpNd H66NU aSRlid"]`
//! - 重定向链接：`/url?q=...&sa=U...` → 还原

use crate::engine::{Engine, EngineContext};
use crate::engines::common::{collapse_whitespace, extract_text};
use crate::error::{SearchError, SearchResult};
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use scraper::Html;
use tracing::debug;

pub struct Google;

#[async_trait]
impl Engine for Google {
    fn id(&self) -> &'static str {
        "google"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let start = (ctx.page.saturating_sub(1)) * 10;
        let q = urlencoding::encode(&ctx.query);
        let hl = locale_to_hl(&ctx.locale);
        let url = format!("https://www.google.com/search?q={q}&hl={hl}&ie=utf8&oe=utf8&filter=0&start={start}");

        let resp = ctx
            .client
            .get(&url)
            .header("Accept", "*/*")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Linux; Android 10; HUAWEI P30 Pro) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36 NSTNWV",
            )
            .header("Cookie", "CONSENT=YES+")
            .send()
            .await?;

        if resp.url().host_str() == Some("sorry.google.com") || resp.url().path().starts_with("/sorry") {
            return Err(SearchError::Captcha("google"));
        }

        let html = resp.text().await?;
        let doc = Html::parse_document(&html);

        let mut results = Vec::new();
        let sel_item = sel!("a[data-ved]:not([class])");
        let sel_title = sel!("div[style]");
        let sel_content = sel!(r#"div[class~="ilUpNd"][class~="H66NU"][class~="aSRlid"]"#);
        // fallback content selectors (google 会频繁微调 class)
        let sel_content_fb = sel!("div.VwiC3b, div.IsZvec, span.st");

        for a in doc.select(&sel_item) {
            let Some(href) = a.value().attr("href") else {
                continue;
            };

            let title_el = a.select(&sel_title).next();
            let title = match title_el {
                Some(e) => extract_text(&e),
                None => continue,
            };
            if title.is_empty() {
                continue;
            }

            // /url?q=...&sa=U... → 还原
            let url_clean = if let Some(stripped) = href.strip_prefix("/url?q=") {
                let head = stripped.split("&sa=U").next().unwrap_or(stripped);
                percent_decode_str(head).decode_utf8_lossy().into_owned()
            } else if href.starts_with("http") {
                href.to_string()
            } else {
                continue;
            };

            // content 要从 result 外层往上找
            let content = a
                .parent()
                .and_then(|p| p.parent())
                .and_then(scraper::ElementRef::wrap)
                .map(|parent_el| {
                    parent_el
                        .select(&sel_content)
                        .chain(parent_el.select(&sel_content_fb))
                        .next()
                        .map(|e| extract_text(&e))
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            results.push(RawResult {
                url: url_clean,
                title,
                content: collapse_whitespace(&content),
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "google", count = results.len(), "parsed");
        Ok(results)
    }
}

fn locale_to_hl(locale: &str) -> &str {
    match locale {
        "all" | "" => "en",
        s if s.starts_with("zh") => "zh-CN",
        s if s.starts_with("ja") => "ja",
        s if s.starts_with("ko") => "ko",
        s if s.starts_with("en") => "en",
        _ => "en",
    }
}
