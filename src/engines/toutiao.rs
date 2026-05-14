//! 今日头条搜索 — searxng 未包含。
//!
//! - **HTTP 路径**：首屏 `window._SSR_DATA` 已被反爬覆盖，基本都会 AuthRequired。
//! - **浏览器路径**（ctx.browser 非空）：走 headless 渲染后从 DOM 抽卡片。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_to_text;
use crate::error::{SearchError, SearchResult};
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use regex::Regex;
use scraper::Html;
use tracing::debug;

pub struct Toutiao;

#[async_trait]
impl Engine for Toutiao {
    fn id(&self) -> &'static str {
        "toutiao"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://so.toutiao.com/search?dvpf=pc&source=input&keyword={q}");

        if let Some(browser) = &ctx.browser {
            let html = browser.fetch_html(&url).await?;
            return parse_toutiao_dom(&html);
        }

        let resp = ctx
            .client
            .get(&url)
            .header("Referer", "https://so.toutiao.com/")
            .send()
            .await?;
        let body = resp.text().await?;

        let re = Regex::new(r"window\._SSR_DATA\s*=\s*(\{[^\n]+?\});?\s*</script>").unwrap();
        let Some(cap) = re.captures(&body) else {
            return Err(SearchError::AuthRequired("toutiao"));
        };
        let json = &cap[1];
        let data: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| SearchError::Engine("toutiao", format!("invalid json: {e}")))?;

        let list = data
            .pointer("/data/data")
            .or_else(|| data.pointer("/data/initialData"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut out = Vec::new();
        for e in list {
            let title = e
                .pointer("/display/title")
                .and_then(|v| v.as_str())
                .or_else(|| e.get("title").and_then(|v| v.as_str()))
                .unwrap_or("");
            let url = e
                .pointer("/display/url")
                .and_then(|v| v.as_str())
                .or_else(|| e.get("url").and_then(|v| v.as_str()))
                .or_else(|| e.get("article_url").and_then(|v| v.as_str()))
                .unwrap_or("");
            let content = e
                .pointer("/display/abstract")
                .and_then(|v| v.as_str())
                .or_else(|| e.get("abstract").and_then(|v| v.as_str()))
                .or_else(|| e.get("content").and_then(|v| v.as_str()))
                .unwrap_or("");
            if title.is_empty() || url.is_empty() {
                continue;
            }
            out.push(RawResult {
                url: url.to_string(),
                title: html_to_text(title),
                content: html_to_text(content),
                ..RawResult::new("", "", "")
            });
        }

        if out.is_empty() {
            return Err(SearchError::AuthRequired("toutiao"));
        }
        debug!(engine = "toutiao", count = out.len(), "parsed (http)");
        Ok(out)
    }
}

fn parse_toutiao_dom(html: &str) -> SearchResult<Vec<RawResult>> {
    // 浏览器渲染后真实数据在 <script data-druid-card-data-id type="application/json">
    // 结构：{"data":{"url":"...", "display":{"emphasized":{"title":"...","summary":"..."},
    //                                         "summary":{"text":"..."}}}}
    let doc = Html::parse_document(html);
    let script_sel = sel!(r#"script[data-druid-card-data-id][type="application/json"]"#);

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in doc.select(&script_sel) {
        let raw = node.text().collect::<String>();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let d = v.get("data").unwrap_or(&v);
        let url = d.get("url").and_then(|x| x.as_str()).unwrap_or_default();
        if url.is_empty() || !url.starts_with("http") {
            continue;
        }
        let title = d
            .pointer("/display/emphasized/title")
            .and_then(|x| x.as_str())
            .or_else(|| d.pointer("/display/title").and_then(|x| x.as_str()))
            .or_else(|| d.get("title").and_then(|x| x.as_str()))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let content = d
            .pointer("/display/emphasized/summary")
            .and_then(|x| x.as_str())
            .or_else(|| d.pointer("/display/summary/text").and_then(|x| x.as_str()))
            .or_else(|| d.get("summary").and_then(|x| x.as_str()))
            .unwrap_or_default();
        if !seen.insert(url.to_string()) {
            continue;
        }
        out.push(RawResult {
            url: url.to_string(),
            title: html_to_text(title),
            content: html_to_text(content),
            ..RawResult::new("", "", "")
        });
    }

    if out.is_empty() {
        return Err(SearchError::Engine(
            "toutiao",
            "browser returned page with no recognizable result cards".into(),
        ));
    }
    debug!(engine = "toutiao", count = out.len(), "parsed (browser)");
    Ok(out)
}
