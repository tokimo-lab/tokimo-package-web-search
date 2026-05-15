//! GitHub 仓库搜索 — JSON API，可选 GITHUB_TOKEN 环境变量提高速率限制。
//!
//! URL: https://api.github.com/search/repositories?q=...
//! 无 token 时 30 req/min，有 token 时 5000 req/hr。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::parse_date_text;
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

pub struct Github;

#[derive(Debug, Deserialize)]
struct Resp {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    full_name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    language: Option<String>,
    created_at: Option<String>,
    pushed_at: Option<String>,
}

#[async_trait]
impl Engine for Github {
    fn id(&self) -> &'static str {
        "github"
    }
    fn weight(&self) -> f64 {
        0.8
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let limit = 10u32;
        let url = format!("https://api.github.com/search/repositories?q={q}&per_page={limit}");

        let mut req = ctx
            .client
            .get(&url)
            .header("User-Agent", "tokimo-web-search")
            .header("Accept", "application/vnd.github+json");

        if let Ok(token) = std::env::var("GITHUB_TOKEN")
            && !token.is_empty()
        {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SearchError::Engine(
                "github",
                format!("HTTP {status}: {body}"),
            ));
        }
        let text = resp.text().await?;
        let data: Resp = serde_json::from_str(&text)
            .map_err(|e| SearchError::Engine("github", format!("invalid json: {e}")))?;

        let mut out = Vec::new();
        for r in data.items {
            let mut parts = Vec::new();
            if let Some(desc) = &r.description
                && !desc.is_empty()
            {
                parts.push(desc.clone());
            }
            parts.push(format!("Stars: {}", r.stargazers_count));
            if let Some(lang) = &r.language {
                parts.push(format!("Language: {lang}"));
            }
            let published_date = r
                .pushed_at
                .as_deref()
                .or(r.created_at.as_deref())
                .and_then(parse_date_text);
            out.push(RawResult {
                url: r.html_url,
                title: r.full_name,
                content: parts.join(" | "),
                published_date,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "github", count = out.len(), "parsed");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_response() {
        let json = r#"{
            "items": [
                {
                    "full_name": "tokushimo/tokimo",
                    "html_url": "https://github.com/tokushimo/tokimo",
                    "description": "A search library",
                    "stargazers_count": 42,
                    "language": "Rust",
                    "created_at": "2024-01-15T10:00:00Z",
                    "pushed_at": "2026-05-10T12:00:00Z"
                },
                {
                    "full_name": "example/repo",
                    "html_url": "https://github.com/example/repo",
                    "description": null,
                    "stargazers_count": 0,
                    "language": null,
                    "created_at": null,
                    "pushed_at": null
                }
            ]
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert_eq!(data.items.len(), 2);
        assert_eq!(data.items[0].full_name, "tokushimo/tokimo");
        assert_eq!(data.items[0].stargazers_count, 42);
        assert!(data.items[1].description.is_none());
        assert!(data.items[1].language.is_none());
    }

    #[test]
    fn content_format_with_all_fields() {
        let item = Item {
            full_name: "org/repo".into(),
            html_url: "https://github.com/org/repo".into(),
            description: Some("A cool project".into()),
            stargazers_count: 100,
            language: Some("Rust".into()),
            created_at: None,
            pushed_at: None,
        };
        let mut parts = Vec::new();
        if let Some(desc) = &item.description
            && !desc.is_empty()
        {
            parts.push(desc.clone());
        }
        parts.push(format!("Stars: {}", item.stargazers_count));
        if let Some(lang) = &item.language {
            parts.push(format!("Language: {lang}"));
        }
        assert_eq!(
            parts.join(" | "),
            "A cool project | Stars: 100 | Language: Rust"
        );
    }

    #[test]
    fn content_format_without_description() {
        let item = Item {
            full_name: "org/repo".into(),
            html_url: "https://github.com/org/repo".into(),
            description: None,
            stargazers_count: 5,
            language: Some("Python".into()),
            created_at: None,
            pushed_at: None,
        };
        let mut parts = Vec::new();
        if let Some(desc) = &item.description
            && !desc.is_empty()
        {
            parts.push(desc.clone());
        }
        parts.push(format!("Stars: {}", item.stargazers_count));
        if let Some(lang) = &item.language {
            parts.push(format!("Language: {lang}"));
        }
        assert_eq!(parts.join(" | "), "Stars: 5 | Language: Python");
    }

    #[test]
    fn empty_items() {
        let json = r#"{"items": []}"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert!(data.items.is_empty());
    }
}
