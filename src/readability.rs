//! Readability 降噪 + 详情抓取（基于 `tokimo-web-fetch`）。

use crate::error::SearchResult;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokimo_web_fetch::DenoisedArticle;

/// 兼容旧 API 的别名 —— 底层就是 [`DenoisedArticle`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailResult {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub byline: Option<String>,
    pub excerpt: Option<String>,
    pub site_name: Option<String>,
    pub lang: Option<String>,
    pub length: usize,
    pub content_html: String,
    pub content_text: String,
}

impl From<DenoisedArticle> for DetailResult {
    fn from(a: DenoisedArticle) -> Self {
        Self {
            url: a.url,
            final_url: a.final_url,
            title: a.title,
            byline: a.byline,
            excerpt: a.excerpt,
            site_name: a.site_name,
            lang: a.lang,
            length: a.length,
            content_html: a.content_html,
            content_text: a.content_text,
        }
    }
}

/// 用给定 reqwest::Client 抓 URL，跑 Readability 降噪。
pub async fn fetch_detail(client: &Client, url: &str) -> SearchResult<DetailResult> {
    let fetcher = tokimo_web_fetch::WebFetcher::builder()
        .http_client(client.clone())
        .build();
    let opts = tokimo_web_fetch::FetchOptions {
        mode: tokimo_web_fetch::FetchMode::Http,
        denoise: tokimo_web_fetch::Denoise::Readability,
        ..Default::default()
    };
    let resp = fetcher.fetch_with(url, &opts).await?;
    let denoised = resp
        .denoised
        .ok_or_else(|| crate::error::SearchError::Readability("missing denoised body".into()))?;
    Ok(DetailResult::from(denoised))
}
