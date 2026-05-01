//! DuckDuckGo HTML — 无需 JS 的纯 HTML 端点。
//!
//! URL: https://html.duckduckgo.com/html/?q=...
//! 结构：`div.result.web-result`，每项 `h2.result__title > a.result__a`（title/href），
//!       `a.result__snippet`（正文摘要），链接经 `//duckduckgo.com/l/?uddg=...` 重定向。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::extract_text;
use crate::error::SearchResult;
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use scraper::Html;
use tracing::debug;
use url::Url;

pub struct DuckDuckGo;

#[async_trait]
impl Engine for DuckDuckGo {
    fn id(&self) -> &'static str {
        "duckduckgo"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let kl = locale_to_kl(&ctx.locale);
        let url = format!("https://html.duckduckgo.com/html/?q={q}&kl={kl}");

        let resp = ctx
            .client
            .get(&url)
            .header("Accept-Language", format!("{},en;q=0.3", ctx.locale))
            .send()
            .await?;
        let html = resp.text().await?;
        let doc = Html::parse_document(&html);

        let sel_item = sel!("div.result.web-result");
        let sel_link = sel!("h2.result__title a.result__a");
        let sel_snippet = sel!("a.result__snippet");

        let mut results = Vec::new();

        for item in doc.select(&sel_item) {
            let Some(a) = item.select(&sel_link).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let title = extract_text(&a);
            if title.is_empty() {
                continue;
            }

            // DuckDuckGo 重定向链接：//duckduckgo.com/l/?uddg=<encoded_url>
            let real = extract_real_url(href);

            let content = item
                .select(&sel_snippet)
                .next()
                .map(|e| extract_text(&e))
                .unwrap_or_default();

            results.push(RawResult::new(real, title, content));
        }

        debug!(engine = "duckduckgo", count = results.len(), "parsed");
        Ok(results)
    }
}

/// 从 DuckDuckGo 重定向链接中提取真实 URL。
/// 格式：`//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&rut=...`
fn extract_real_url(href: &str) -> String {
    // 补全协议相对 URL
    let full = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href.to_string()
    };

    if let Ok(parsed) = Url::parse(&full)
        && let Some(uddg) = parsed
            .query_pairs()
            .find(|(k, _)| k == "uddg")
            .map(|(_, v)| v.into_owned())
        && !uddg.is_empty()
    {
        return uddg;
    }

    // fallback：原样返回
    full
}

fn locale_to_kl(locale: &str) -> &str {
    match locale {
        s if s.starts_with("zh-CN") || s == "zh" => "cn-zh",
        s if s.starts_with("zh-TW") => "tw-tzh",
        s if s.starts_with("zh") => "hk-tzh",
        s if s.starts_with("en-GB") => "uk-en",
        s if s.starts_with("en") => "us-en",
        s if s.starts_with("ja") => "jp-jp",
        s if s.starts_with("ko") => "kr-kr",
        s if s.starts_with("de") => "de-de",
        s if s.starts_with("fr") => "fr-fr",
        s if s.starts_with("es") => "es-es",
        s if s.starts_with("ru") => "ru-ru",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_real_url_with_uddg() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc123";
        assert_eq!(extract_real_url(href), "https://example.com/page");
    }

    #[test]
    fn extract_real_url_without_uddg() {
        let href = "https://example.com/page";
        assert_eq!(extract_real_url(href), "https://example.com/page");
    }

    #[test]
    fn extract_real_url_relative_protocol() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Ffoo.bar";
        assert_eq!(extract_real_url(href), "https://foo.bar");
    }

    #[test]
    fn locale_mapping() {
        assert_eq!(locale_to_kl("zh-CN"), "cn-zh");
        assert_eq!(locale_to_kl("en-US"), "us-en");
        assert_eq!(locale_to_kl("ja-JP"), "jp-jp");
        assert_eq!(locale_to_kl("xx-YY"), "");
    }
}
