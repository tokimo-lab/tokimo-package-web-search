//! CSDN 博客搜索 — JSON API，无需 API key。
//!
//! URL: https://so.csdn.net/api/v3/search?q=...&p=...
//! 分页：page-number 循环，直到收集够结果或返回空。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::{html_to_text, parse_date_text};
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

pub struct Csdn;

#[derive(Debug, Deserialize)]
struct Resp {
    result_vos: Option<Vec<Item>>,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: Option<String>,
    url_location: Option<String>,
    digest: Option<String>,
    nickname: Option<String>,
    created_at: Option<String>,
}

#[async_trait]
impl Engine for Csdn {
    fn id(&self) -> &'static str {
        "csdn"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let mut page = ctx.page.max(1);
        let limit = 10usize;
        let mut out = Vec::new();

        loop {
            let url = format!("https://so.csdn.net/api/v3/search?q={q}&p={page}");

            let resp = ctx
                .client
                .get(&url)
                .header("User-Agent", "Apifox/1.0.0 (https://apifox.com)")
                .header("Accept", "*/*")
                .header("Host", "so.csdn.net")
                .send()
                .await?;
            let text = resp.text().await?;
            let data: Resp = serde_json::from_str(&text)
                .map_err(|e| SearchError::Engine("csdn", format!("invalid json: {e}")))?;

            let items = match data.result_vos {
                Some(v) if !v.is_empty() => v,
                _ => break,
            };

            for it in items {
                let Some(url) = it.url_location.filter(|u| !u.is_empty()) else {
                    continue;
                };
                let title = it.title.unwrap_or_default();
                let content = it.digest.map(|d| html_to_text(&d)).unwrap_or_default();
                let published_date = it.created_at.as_deref().and_then(parse_date_text);
                out.push(RawResult {
                    url,
                    title,
                    content,
                    author: it.nickname,
                    published_date,
                    ..RawResult::new("", "", "")
                });
            }

            if out.len() >= limit {
                break;
            }
            page += 1;
        }

        debug!(engine = "csdn", count = out.len(), "parsed");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response() {
        let json = r#"{
            "result_vos": [
                {
                    "title": "Rust 入门教程",
                    "url_location": "https://blog.csdn.net/user/article/123",
                    "digest": "本文介绍 <em>Rust</em> 基础",
                    "nickname": "test_user",
                    "created_at": "2025-10-11 03:53:32"
                }
            ]
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        let items = data.result_vos.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Rust 入门教程"));
        assert_eq!(
            items[0].url_location.as_deref(),
            Some("https://blog.csdn.net/user/article/123")
        );
        assert_eq!(items[0].nickname.as_deref(), Some("test_user"));
    }

    #[test]
    fn parse_null_result_vos() {
        let json = r#"{"result_vos": null}"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert!(data.result_vos.is_none());
    }

    #[test]
    fn filter_empty_url() {
        let json = r#"{
            "result_vos": [
                { "title": "A", "url_location": "", "digest": "x", "nickname": "u" },
                { "title": "B", "url_location": "https://blog.csdn.net/1", "digest": "y", "nickname": "v" }
            ]
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        let items = data.result_vos.unwrap();
        let valid: Vec<_> = items
            .iter()
            .filter(|it| it.url_location.as_deref().is_some_and(|u| !u.is_empty()))
            .collect();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].title.as_deref(), Some("B"));
    }

    #[test]
    fn digest_html_stripped() {
        let digest = "这是 <strong>加粗</strong> 和 <em>斜体</em>";
        let plain = crate::engines::common::html_to_text(digest);
        assert_eq!(plain, "这是 加粗 和 斜体");
    }

    #[test]
    fn empty_items() {
        let json = r#"{"result_vos": []}"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert!(data.result_vos.unwrap().is_empty());
    }
}
