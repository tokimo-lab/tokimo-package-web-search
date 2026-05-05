//! GitHub 代码搜索 — JSON API，可选 GITHUB_TOKEN 环境变量提高速率限制。
//!
//! URL: https://api.github.com/search/code?q=...
//! 无 token 时 10 req/min，有 token 时 5000 req/hr。

use crate::engine::{Engine, EngineContext};
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

pub struct GithubCode;

#[derive(Debug, Deserialize)]
struct Resp {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    name: String,
    html_url: String,
    path: String,
    repository: Repo,
}

#[derive(Debug, Deserialize)]
struct Repo {
    full_name: String,
}

#[async_trait]
impl Engine for GithubCode {
    fn id(&self) -> &'static str {
        "github-code"
    }
    fn weight(&self) -> f64 {
        0.8
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let limit = 10u32;
        let url = format!("https://api.github.com/search/code?q={q}&per_page={limit}");

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
                "github-code",
                format!("HTTP {status}: {body}"),
            ));
        }
        let text = resp.text().await?;
        let data: Resp = serde_json::from_str(&text)
            .map_err(|e| SearchError::Engine("github-code", format!("invalid json: {e}")))?;

        let mut out = Vec::new();
        for r in data.items {
            let title = format!("{} -- {}", r.repository.full_name, r.name);
            let content = format!("Path: {}", r.path);
            out.push(RawResult::new(r.html_url, title, content));
        }

        debug!(engine = "github-code", count = out.len(), "parsed");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_response() {
        let json = r#"{
            "items": [
                {
                    "name": "lib.rs",
                    "html_url": "https://github.com/org/repo/blob/main/src/lib.rs",
                    "path": "src/lib.rs",
                    "repository": { "full_name": "org/repo" }
                },
                {
                    "name": "main.rs",
                    "html_url": "https://github.com/org/repo/blob/main/src/main.rs",
                    "path": "src/main.rs",
                    "repository": { "full_name": "org/repo" }
                }
            ]
        }"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert_eq!(data.items.len(), 2);
        assert_eq!(data.items[0].name, "lib.rs");
        assert_eq!(data.items[0].repository.full_name, "org/repo");
    }

    #[test]
    fn title_format() {
        let item = Item {
            name: "lib.rs".into(),
            html_url: "https://github.com/a/b/blob/main/src/lib.rs".into(),
            path: "src/lib.rs".into(),
            repository: Repo {
                full_name: "a/b".into(),
            },
        };
        let title = format!("{} -- {}", item.repository.full_name, item.name);
        assert_eq!(title, "a/b -- lib.rs");
    }

    #[test]
    fn content_format() {
        let item = Item {
            name: "main.rs".into(),
            html_url: "https://github.com/x/y/blob/main/main.rs".into(),
            path: "main.rs".into(),
            repository: Repo {
                full_name: "x/y".into(),
            },
        };
        let content = format!("Path: {}", item.path);
        assert_eq!(content, "Path: main.rs");
    }

    #[test]
    fn empty_items() {
        let json = r#"{"items": []}"#;
        let data: Resp = serde_json::from_str(json).unwrap();
        assert!(data.items.is_empty());
    }
}
