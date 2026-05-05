//! LinuxDo 论坛搜索 — 元引擎，委托给 Bing/DDG + site:linux.do 前缀过滤。
//!
//! 策略：在查询前加 `site:linux.do`，用 Bing 搜索后过滤 URL 域名。
//! Bing 无结果时回退到 DuckDuckGo。

use crate::engine::{Engine, EngineContext};
use crate::engines::{bing, duckduckgo};
use crate::error::SearchResult;
use crate::types::RawResult;
use async_trait::async_trait;
use tracing::debug;
use url::Url;

pub struct LinuxDo;

#[async_trait]
impl Engine for LinuxDo {
    fn id(&self) -> &'static str {
        "linuxdo"
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let site_query = format!("site:linux.do {}", ctx.query);
        let mut sub_ctx = ctx.clone();
        sub_ctx.query = site_query;

        // 尝试 Bing
        let bing_results = bing::Bing.search(&sub_ctx).await.unwrap_or_default();
        let results = if bing_results.is_empty() {
            // 回退 DuckDuckGo
            duckduckgo::DuckDuckGo
                .search(&sub_ctx)
                .await
                .unwrap_or_default()
        } else {
            bing_results
        };

        let limit = 10usize;
        let out: Vec<RawResult> = results
            .into_iter()
            .filter(|r| {
                Url::parse(&r.url)
                    .ok()
                    .and_then(|u| u.host_str().map(is_linuxdo_host))
                    .unwrap_or(false)
            })
            .take(limit)
            .collect();

        debug!(engine = "linuxdo", count = out.len(), "parsed");
        Ok(out)
    }
}

fn is_linuxdo_host(host: &str) -> bool {
    host == "linux.do" || host.ends_with(".linux.do")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_exact_match() {
        assert!(is_linuxdo_host("linux.do"));
    }

    #[test]
    fn host_subdomain() {
        assert!(is_linuxdo_host("www.linux.do"));
        assert!(is_linuxdo_host("bbs.linux.do"));
    }

    #[test]
    fn host_reject_other() {
        assert!(!is_linuxdo_host("example.com"));
        assert!(!is_linuxdo_host("linux.com"));
        assert!(!is_linuxdo_host("notlinux.do"));
    }

    #[test]
    fn host_reject_empty() {
        assert!(!is_linuxdo_host(""));
    }
}
