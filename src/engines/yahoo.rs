//! Yahoo Web — 参考 searxng `engines/yahoo.py`
//!
//! URL: https://search.yahoo.com/search?p=...&b=<start>
//! 结构：`div.algo-sr`，title 在 `div.compTitle h3 a`，url 是 `/RU=<real>/RK...`

use crate::engine::{Engine, EngineContext};
use crate::engines::common::{extract_text, html_to_text};
use crate::error::SearchResult;
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use scraper::Html;
use tracing::debug;

pub struct Yahoo;

#[async_trait]
impl Engine for Yahoo {
    fn id(&self) -> &'static str {
        "yahoo"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let b = ((ctx.page.saturating_sub(1)) * 10) + 1;
        let url = format!("https://search.yahoo.com/search?p={q}&b={b}");

        let resp = ctx
            .client
            .get(&url)
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .await?;
        let html = resp.text().await?;
        let doc = Html::parse_document(&html);

        let sel_item = sel!("div.algo-sr");
        // 两套 selector（匹配 search.yahoo.com vs 其他地域）
        let sel_link_a = sel!(r"div.compTitle h3 a");
        let sel_link_b = sel!(r"div.compTitle a");
        let sel_content = sel!(r"div.compText");

        let mut results = Vec::new();
        for item in doc.select(&sel_item) {
            let link = item
                .select(&sel_link_a)
                .next()
                .or_else(|| item.select(&sel_link_b).next());
            let Some(a) = link else { continue };
            let Some(raw_url) = a.value().attr("href") else {
                continue;
            };
            let title_raw = a
                .value()
                .attr("aria-label")
                .map_or_else(|| extract_text(&a), String::from);
            let title = html_to_text(&title_raw);

            let content = item
                .select(&sel_content)
                .next()
                .map(|e| extract_text(&e))
                .unwrap_or_default();
            let url_clean = parse_yahoo_url(raw_url);

            if title.is_empty() || url_clean.is_empty() {
                continue;
            }
            results.push(RawResult {
                url: url_clean,
                title,
                content,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "yahoo", count = results.len(), "parsed");
        Ok(results)
    }
}

fn parse_yahoo_url(s: &str) -> String {
    // 提取 /RU=<url>/RK 之间的部分
    let Some(ru_idx) = s.find("/RU=") else {
        return s.to_string();
    };
    let after = &s[ru_idx + 4..];
    let end = ["/RS", "/RK"]
        .iter()
        .filter_map(|m| after.find(m))
        .min()
        .unwrap_or(after.len());
    let slice = &after[..end];
    percent_decode_str(slice).decode_utf8_lossy().into_owned()
}
