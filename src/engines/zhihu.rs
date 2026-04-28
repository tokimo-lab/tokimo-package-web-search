//! 知乎搜索 — searxng 未包含。
//!
//! 知乎的 `search_v3` API 需要登录 cookie，未登录时 HTML 页面内容是 SSR 骨架，
//! 真正的结果通过 `/api/v4/search_v3` 异步拉取且需要 x-zse-96 签名。
//! 本实现尽力从 HTML 中解析 `<script id="js-initialData">` 初始数据，
//! 如果被重定向到登录或数据缺失则返回 `AuthRequired`。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_to_text;
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use scraper::Html;
use tracing::debug;

pub struct Zhihu;

#[async_trait]
impl Engine for Zhihu {
    fn id(&self) -> &'static str {
        "zhihu"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://www.zhihu.com/search?type=content&q={q}");
        let resp = ctx
            .client
            .get(&url)
            .header("Referer", "https://www.zhihu.com/")
            .send()
            .await?;
        if resp.url().path().contains("signin") || resp.url().path().contains("unhuman") {
            return Err(SearchError::AuthRequired("zhihu"));
        }
        let body = resp.text().await?;

        // 先尝试 initialData
        if let Some(items) = extract_initial_data(&body) {
            debug!(engine = "zhihu", count = items.len(), "initialData");
            return Ok(items);
        }

        // fallback：HTML selector
        let doc = Html::parse_document(&body);
        let sel_item = crate::sel!(r"div.SearchResult-Card, div.Card.SearchResult-Card");
        let sel_a = crate::sel!(r"h2 a, .ContentItem-title a");
        let sel_content =
            crate::sel!(r".RichContent-inner, .CopyrightRichText-richText, .RichText");

        let mut out = Vec::new();
        for it in doc.select(&sel_item) {
            let Some(a) = it.select(&sel_a).next() else {
                continue;
            };
            let title = html_to_text(&a.inner_html());
            let href = a.value().attr("href").unwrap_or_default();
            let url = normalize_zhihu_url(href);
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let content = it
                .select(&sel_content)
                .next()
                .map(|e| html_to_text(&e.inner_html()))
                .unwrap_or_default();
            out.push(RawResult {
                url,
                title,
                content,
                ..RawResult::new("", "", "")
            });
        }

        if out.is_empty() {
            return Err(SearchError::AuthRequired("zhihu"));
        }
        Ok(out)
    }
}

fn normalize_zhihu_url(href: &str) -> String {
    if href.starts_with("//") {
        format!("https:{href}")
    } else if href.starts_with('/') {
        format!("https://www.zhihu.com{href}")
    } else {
        href.to_string()
    }
}

fn extract_initial_data(html: &str) -> Option<Vec<RawResult>> {
    let start = html.find(r#"<script id="js-initialData""#)?;
    let after = &html[start..];
    let open = after.find('>')? + 1;
    let close = after[open..].find("</script>")?;
    let json = &after[open..open + close];
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let list = v
        .pointer("/initialState/search/searchResult")
        .or_else(|| v.pointer("/initialState/search/searchResultByTab/general/data"))?
        .as_array()?;
    let mut out = Vec::new();
    for entry in list {
        let obj = entry.get("object").unwrap_or(entry);
        let title = obj
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| {
                obj.get("question")
                    .and_then(|q| q.get("name"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        let content = obj
            .get("excerpt")
            .or_else(|| obj.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let url = url.replace("api.zhihu.com/answers/", "www.zhihu.com/answer/");
        out.push(RawResult {
            url,
            title: html_to_text(title),
            content: html_to_text(content),
            ..RawResult::new("", "", "")
        });
    }
    if out.is_empty() { None } else { Some(out) }
}
