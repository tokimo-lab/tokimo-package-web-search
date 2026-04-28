//! 百度网页搜索 — 参考 searxng `engines/baidu.py`
//!
//! URL: https://www.baidu.com/s?wd=...&rn=10&pn=...&tn=json
//! 返回 JSON：`data.feed.entry[]`

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_unescape;
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use chrono::TimeZone;
use serde::Deserialize;
use tracing::debug;

pub struct Baidu;

#[derive(Debug, Deserialize)]
struct Resp {
    feed: Option<Feed>,
}

#[derive(Debug, Deserialize)]
struct Feed {
    entry: Option<Vec<Entry>>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    title: Option<String>,
    url: Option<String>,
    abs: Option<String>,
    time: Option<i64>,
}

#[async_trait]
impl Engine for Baidu {
    fn id(&self) -> &'static str {
        "baidu"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let page = ctx.page.max(1);
        let pn = (page - 1) * 10;
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://www.baidu.com/s?wd={q}&rn=10&pn={pn}&tn=json");

        let resp = ctx.client.get(&url).send().await?;

        if let Some(loc) = resp.headers().get("location")
            && loc
                .to_str()
                .unwrap_or_default()
                .contains("wappass.baidu.com/static/captcha")
        {
            return Err(SearchError::Captcha("baidu"));
        }

        let text = resp.text().await?;
        let data: Resp =
            serde_json::from_str(&text).map_err(|e| SearchError::Engine("baidu", format!("invalid json: {e}")))?;

        let mut out = Vec::new();
        let Some(feed) = data.feed else {
            return Err(SearchError::Engine("baidu", "missing feed".into()));
        };
        let Some(entries) = feed.entry else {
            return Ok(out);
        };
        for e in entries {
            let (Some(title), Some(url)) = (e.title, e.url) else {
                continue;
            };
            let content = e.abs.unwrap_or_default();
            let dt = e.time.and_then(|t| chrono::Utc.timestamp_opt(t, 0).single());
            out.push(RawResult {
                url,
                title: html_unescape(&title),
                content: html_unescape(&content),
                published_date: dt,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "baidu", count = out.len(), "parsed");
        Ok(out)
    }
}
