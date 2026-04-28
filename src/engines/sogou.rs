//! 搜狗 — 参考 searxng `engines/sogou.py`
//!
//! URL: https://www.sogou.com/web?query=&page=
//! 注意：302 到 antispider 表示触发反爬。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::extract_text;
use crate::error::{SearchError, SearchResult};
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use regex::Regex;
use scraper::Html;
use tracing::debug;

pub struct Sogou;

#[async_trait]
impl Engine for Sogou {
    fn id(&self) -> &'static str {
        "sogou"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let page = ctx.page.max(1);
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://www.sogou.com/web?query={q}&page={page}");

        let resp = ctx.client.get(&url).send().await?;
        let final_url = resp.url().as_str().to_string();
        if final_url.contains("antispider") {
            return Err(SearchError::Captcha("sogou"));
        }
        let body = resp.text().await?;
        let doc = Html::parse_document(&body);

        // rb / vrwrap 两类结果块
        let sel_items = sel!(r"div.rb, div.vrwrap");
        let sel_a1 = sel!(r"h3.pt a");
        let sel_a2 = sel!(r"h3.vr-title a");
        let sel_ft = sel!(r"div.ft");
        let sel_attr = sel!(r"div.attribute-centent, div.fz-mid.space-txt");

        let re_data_url = Regex::new(r#"data-url="([^"]+)""#).unwrap();

        let mut results = Vec::new();
        for item in doc.select(&sel_items) {
            // 跳过 special-wrap
            if item.select(&sel!(r"div.special-wrap")).next().is_some() {
                continue;
            }
            let link = item.select(&sel_a1).next().or_else(|| item.select(&sel_a2).next());
            let Some(a) = link else { continue };
            let title = extract_text(&a);
            let href_raw = a.value().attr("href").unwrap_or_default();

            // /link?url=... 需要从 data-url 取真实 URL
            let href = if href_raw.starts_with("/link?url=") {
                let item_html = item.html();
                re_data_url.captures(&item_html).and_then(|c| c.get(1)).map_or_else(
                    || format!("https://www.sogou.com{href_raw}"),
                    |m| m.as_str().to_string(),
                )
            } else {
                href_raw.to_string()
            };

            if title.is_empty() || href.is_empty() {
                continue;
            }

            let content = item
                .select(&sel_ft)
                .next()
                .or_else(|| item.select(&sel_attr).next())
                .map(|e| extract_text(&e))
                .unwrap_or_default();

            results.push(RawResult {
                url: href,
                title,
                content,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "sogou", count = results.len(), "parsed");
        Ok(results)
    }
}
