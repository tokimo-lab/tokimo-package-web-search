//! 抖音搜索 — searxng 未包含。
//!
//! 抖音 Web 搜索完全由 JS 驱动，接口带 X-Bogus/`_signature` 签名 + webid cookie + verifyFp，
//! 没有浏览器环境极难复现。本 crate 不跑 headless 浏览器，因此这里直接
//! 返回 `AuthRequired`，告诉上层：需要登录 cookie 或 headless 方案。

use crate::engine::{Engine, EngineContext};
use crate::error::{SearchError, SearchResult};
use crate::types::RawResult;
use async_trait::async_trait;

pub struct Douyin;

#[async_trait]
impl Engine for Douyin {
    fn id(&self) -> &'static str {
        "douyin"
    }
    fn is_china(&self) -> bool {
        true
    }
    fn category(&self) -> &'static str {
        "videos"
    }

    async fn search(&self, _ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        Err(SearchError::AuthRequired("douyin"))
    }
}
