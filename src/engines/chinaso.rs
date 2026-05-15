//! ChinaSo 新闻 — 参考 searxng `engines/chinaso.py`
//!
//! URL: https://www.chinaso.com/v5/general/v1/web/search?q=&pn=&ps=10
//! 需要随机 uid cookie。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_to_text;
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use base64::Engine as _;
use chrono::TimeZone;
use rand::RngCore;
use serde::Deserialize;
use tracing::debug;

pub struct ChinaSo;

#[derive(Debug, Deserialize)]
struct Resp {
    data: Option<Inner>,
}

#[derive(Debug, Deserialize)]
struct Inner {
    data: Option<Vec<Entry>>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    title: Option<String>,
    url: Option<String>,
    snippet: Option<String>,
    timestamp: Option<serde_json::Value>,
}

#[async_trait]
impl Engine for ChinaSo {
    fn id(&self) -> &'static str {
        "chinaso"
    }
    fn warmup_url(&self) -> Option<&str> {
        Some("https://www.chinaso.com/")
    }
    fn is_china(&self) -> bool {
        true
    }
    fn category(&self) -> &'static str {
        "news"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let page = ctx.page.max(1);
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://www.chinaso.com/v5/general/v1/web/search?q={q}&pn={page}&ps=10");

        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        let uid = base64::engine::general_purpose::STANDARD.encode(bytes);

        let resp = ctx
            .client
            .get(&url)
            .header("Cookie", format!("uid={uid}"))
            .send()
            .await?;
        let text = resp.text().await?;
        let data: Resp = serde_json::from_str(&text)
            .map_err(|e| SearchError::Engine("chinaso", format!("invalid json: {e}")))?;

        let entries = data.data.and_then(|d| d.data).unwrap_or_default();
        let mut out = Vec::new();
        for e in entries {
            let (Some(title), Some(url)) = (e.title, e.url) else {
                continue;
            };
            let snippet = e.snippet.unwrap_or_default();
            let ts = e.timestamp.and_then(|v| match v {
                serde_json::Value::Number(n) => n.as_i64(),
                serde_json::Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let dt = ts.and_then(|t| chrono::Utc.timestamp_opt(t, 0).single());
            out.push(RawResult {
                url,
                title: html_to_text(&title),
                content: html_to_text(&snippet),
                published_date: dt,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "chinaso", count = out.len(), "parsed");
        Ok(out)
    }
}
