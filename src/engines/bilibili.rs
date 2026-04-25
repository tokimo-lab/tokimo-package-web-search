//! Bilibili 视频搜索 — 参考 searxng `engines/bilibili.py`
//!
//! URL: https://api.bilibili.com/x/web-interface/search/type?keyword=&search_type=video
//! 需要一个随机 buvid3 cookie。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::html_to_text;
use crate::error::{SearchError, SearchResult};
use crate::types::{RawResult, ResultTemplate};
use async_trait::async_trait;
use chrono::TimeZone;
use rand::Rng;
use serde::Deserialize;
use tracing::debug;

pub struct Bilibili;

#[derive(Debug, Deserialize)]
struct Resp {
    data: Option<Data>,
}

#[derive(Debug, Deserialize)]
struct Data {
    result: Option<Vec<Item>>,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: String,
    arcurl: String,
    pic: Option<String>,
    description: Option<String>,
    author: Option<String>,
    aid: serde_json::Value,
    pubdate: Option<i64>,
}

#[async_trait]
impl Engine for Bilibili {
    fn id(&self) -> &'static str {
        "bilibili"
    }
    fn is_china(&self) -> bool {
        true
    }
    fn category(&self) -> &'static str {
        "videos"
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let page = ctx.page.max(1);
        let q = urlencoding::encode(&ctx.query);
        let url = format!(
            "https://api.bilibili.com/x/web-interface/search/type?__refresh__=true&page={page}&page_size=20&single_column=0&keyword={q}&search_type=video"
        );

        let buvid3 = {
            use std::fmt::Write;
            let mut rng = rand::thread_rng();
            let mut hex = String::with_capacity(16);
            for _ in 0..16 {
                write!(&mut hex, "{:X}", rng.gen_range(0..16)).expect("write to String");
            }
            format!("{hex}infoc")
        };
        let cookie = format!(
            "innersign=0; buvid3={buvid3}; i-wanna-go-back=-1; b_ut=7; FEED_LIVE_VERSION=V8; header_theme_version=undefined; home_feed_column=4"
        );

        let resp = ctx
            .client
            .get(&url)
            .header("Referer", "https://www.bilibili.com")
            .header("Cookie", cookie)
            .send()
            .await?;
        let text = resp.text().await?;
        let data: Resp = serde_json::from_str(&text)
            .map_err(|e| SearchError::Engine("bilibili", format!("invalid json: {e}")))?;

        let items = data.data.and_then(|d| d.result).unwrap_or_default();
        let mut out = Vec::new();
        for it in items {
            let aid_str = match &it.aid {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            let iframe = if aid_str.is_empty() {
                None
            } else {
                Some(format!(
                    "https://player.bilibili.com/player.html?aid={aid_str}&high_quality=1&autoplay=false&danmaku=0"
                ))
            };
            let dt = it
                .pubdate
                .and_then(|t| chrono::Utc.timestamp_opt(t, 0).single());
            out.push(RawResult {
                url: it.arcurl,
                title: html_to_text(&it.title),
                content: it.description.unwrap_or_default(),
                template: ResultTemplate::Videos,
                thumbnail: it.pic,
                img_src: None,
                iframe_src: iframe,
                author: it.author,
                published_date: dt,
            });
        }

        debug!(engine = "bilibili", count = out.len(), "parsed");
        Ok(out)
    }
}
