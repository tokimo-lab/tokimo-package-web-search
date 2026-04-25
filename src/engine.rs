use crate::error::SearchResult;
use crate::types::RawResult;
use async_trait::async_trait;
use reqwest::Client;
use std::sync::Arc;

/// 引擎调用上下文：复用 HTTP 客户端，提供 UA/locale/page 等参数。
#[derive(Clone)]
pub struct EngineContext {
    pub client: Client,
    pub query: String,
    pub page: u32,
    /// IETF BCP47 语言标签，例如 "zh-CN" / "en-US" / "all"
    pub locale: String,
    /// 安全搜索级别：0=off, 1=moderate, 2=strict（与 searxng 一致）
    pub safesearch: u8,
    pub user_agent: String,
    /// 可选 headless 浏览器；纯 JS / 强反爬站（toutiao / zhihu / douyin / google）
    /// 在注入时才能拿到真实结果。见 [`crate::browser`]。
    pub browser: Option<Arc<dyn crate::browser::BrowserFetch>>,
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// 引擎唯一标识（kebab-case）
    fn id(&self) -> &'static str;

    /// 排序权重，与 searxng 的 engine.weight 一致，默认 1.0
    fn weight(&self) -> f64 {
        1.0
    }

    /// 是否为国内引擎（用来做区域筛选）
    fn is_china(&self) -> bool {
        false
    }

    /// 引擎返回的结果类别（用来前端分组，也影响 template）
    fn category(&self) -> &'static str {
        "general"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>>;
}
