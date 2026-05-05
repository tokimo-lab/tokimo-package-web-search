//! 掘金文章搜索 — POST JSON API，cursor 分页。
//!
//! URL: https://api.juejin.cn/search_api/v1/article/search
//! Body: { key_word, limit: 10, cursor: "0", sort_type: 0 }

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_to_text;
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

pub struct Juejin;

#[derive(Debug, Serialize)]
struct ReqBody<'a> {
    key_word: &'a str,
    limit: u32,
    cursor: &'a str,
    sort_type: u32,
}

#[derive(Debug, Deserialize)]
struct Resp {
    data: Option<Vec<Item>>,
    cursor: Option<String>,
    has_more: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Item {
    result_model: ResultModel,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResultModel {
    article_id: String,
    article_info: ArticleInfo,
    author_user_info: AuthorInfo,
}

#[derive(Debug, Deserialize)]
struct ArticleInfo {
    brief_content: Option<String>,
    view_count: Option<u64>,
    digg_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AuthorInfo {
    user_name: Option<String>,
}

#[async_trait]
impl Engine for Juejin {
    fn id(&self) -> &'static str {
        "juejin"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let limit = 10u32;
        let mut cursor = "0".to_string();
        let mut out = Vec::new();

        loop {
            let body = ReqBody {
                key_word: &ctx.query,
                limit,
                cursor: &cursor,
                sort_type: 0,
            };

            let resp = ctx
                .client
                .post("https://api.juejin.cn/search_api/v1/article/search")
                .json(&body)
                .send()
                .await?;
            let text = resp.text().await?;
            let data: Resp = serde_json::from_str(&text)
                .map_err(|e| SearchError::Engine("juejin", format!("invalid json: {e}")))?;

            let items = data.data.unwrap_or_default();
            for it in items {
                let Some(title) = it.title.filter(|t| !t.is_empty()) else {
                    continue;
                };
                let url = format!("https://juejin.cn/post/{}", it.result_model.article_id);
                let ai = &it.result_model.article_info;
                let mut parts = Vec::new();
                if let Some(brief) = &ai.brief_content {
                    parts.push(html_to_text(brief));
                }
                if let Some(v) = ai.view_count {
                    parts.push(format!("views: {v}"));
                }
                if let Some(d) = ai.digg_count {
                    parts.push(format!("likes: {d}"));
                }
                out.push(RawResult {
                    url,
                    title,
                    content: parts.join(" | "),
                    author: it.result_model.author_user_info.user_name,
                    ..RawResult::new("", "", "")
                });
            }

            if out.len() >= limit as usize {
                break;
            }
            if data.has_more != Some(true) {
                break;
            }
            match data.cursor {
                Some(c) if !c.is_empty() => cursor = c,
                _ => break,
            }
        }

        debug!(engine = "juejin", count = out.len(), "parsed");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response() {
        let json = r#"{
            "data": [
                {
                    "title": "Rust 异步编程",
                    "result_model": {
                        "article_id": "7123456789",
                        "article_info": {
                            "brief_content": "深入理解 tokio",
                            "view_count": 5000,
                            "digg_count": 120
                        },
                        "author_user_info": {
                            "user_name": "rustacean"
                        }
                    }
                }
            ],
            "cursor": "10",
            "has_more": true
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert_eq!(data.data.as_ref().unwrap().len(), 1);
        assert_eq!(data.cursor.as_deref(), Some("10"));
        assert_eq!(data.has_more, Some(true));

        let item = &data.data.unwrap()[0];
        assert_eq!(item.title.as_deref(), Some("Rust 异步编程"));
        assert_eq!(item.result_model.article_id, "7123456789");
        assert_eq!(
            item.result_model.article_info.brief_content.as_deref(),
            Some("深入理解 tokio")
        );
        assert_eq!(
            item.result_model.author_user_info.user_name.as_deref(),
            Some("rustacean")
        );
    }

    #[test]
    fn url_construction() {
        let article_id = "7123456789";
        let url = format!("https://juejin.cn/post/{article_id}");
        assert_eq!(url, "https://juejin.cn/post/7123456789");
    }

    #[test]
    fn content_format_full() {
        let ai = ArticleInfo {
            brief_content: Some("简介".into()),
            view_count: Some(1000),
            digg_count: Some(50),
        };
        let mut parts = Vec::new();
        if let Some(brief) = &ai.brief_content {
            parts.push(crate::engines::common::html_to_text(brief));
        }
        if let Some(v) = ai.view_count {
            parts.push(format!("views: {v}"));
        }
        if let Some(d) = ai.digg_count {
            parts.push(format!("likes: {d}"));
        }
        assert_eq!(parts.join(" | "), "简介 | views: 1000 | likes: 50");
    }

    #[test]
    fn content_format_no_optional() {
        let ai = ArticleInfo {
            brief_content: None,
            view_count: None,
            digg_count: None,
        };
        let mut parts = Vec::new();
        if let Some(brief) = &ai.brief_content {
            parts.push(crate::engines::common::html_to_text(brief));
        }
        if let Some(v) = ai.view_count {
            parts.push(format!("views: {v}"));
        }
        if let Some(d) = ai.digg_count {
            parts.push(format!("likes: {d}"));
        }
        assert!(parts.is_empty());
    }

    #[test]
    fn empty_data() {
        let json = r#"{"data": [], "cursor": "", "has_more": false}"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert!(data.data.unwrap().is_empty());
        assert_eq!(data.has_more, Some(false));
    }

    #[test]
    fn skip_empty_title() {
        let json = r#"{
            "data": [
                {
                    "title": "",
                    "result_model": {
                        "article_id": "1",
                        "article_info": { "brief_content": "x", "view_count": 1, "digg_count": 1 },
                        "author_user_info": { "user_name": "u" }
                    }
                },
                {
                    "title": "Good Title",
                    "result_model": {
                        "article_id": "2",
                        "article_info": { "brief_content": "y", "view_count": 2, "digg_count": 2 },
                        "author_user_info": { "user_name": "v" }
                    }
                }
            ],
            "has_more": false
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        let valid: Vec<_> = data
            .data
            .unwrap()
            .into_iter()
            .filter(|it| it.title.as_deref().is_some_and(|t| !t.is_empty()))
            .collect();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].title.as_deref(), Some("Good Title"));
    }
}
