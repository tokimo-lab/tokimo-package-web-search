//! Bing Web — 参考 searxng `engines/bing.py`
//!
//! URL: https://www.bing.com/search?q=...
//! 结构：`ol#b_results > li.b_algo`, 每项 `h2 > a`（title/href），`p` 含正文
//! 部分链接被 base64 重编码：`https://www.bing.com/ck/a?...&u=a1<b64>`

use crate::engine::{Engine, EngineContext};
use crate::engines::common::{extract_text, parse_date_text};
use crate::error::SearchResult;
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use base64::Engine as _;
use scraper::Html;
use tracing::debug;
use url::Url;

pub struct Bing;

#[async_trait]
impl Engine for Bing {
    fn id(&self) -> &'static str {
        "bing"
    }
    fn warmup_url(&self) -> Option<&str> {
        Some("https://www.bing.com/")
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let q = urlencoding::encode(&ctx.query);
        let mkt = locale_to_mkt(&ctx.locale);
        let url = format!("https://www.bing.com/search?q={q}&mkt={mkt}");

        let resp = ctx
            .client
            .get(&url)
            .header("Accept-Language", format!("{mkt},en;q=0.3"))
            .send()
            .await?;
        let html = resp.text().await?;
        let doc = Html::parse_document(&html);

        let sel_item = sel!("ol#b_results > li.b_algo");
        let sel_link = sel!("h2 a");
        let sel_content = sel!("p");
        let sel_date = sel!("p.b_lineclamp2");

        let mut results = Vec::new();

        for item in doc.select(&sel_item) {
            let Some(a) = item.select(&sel_link).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let title = extract_text(&a);
            if title.is_empty() {
                continue;
            }

            // Bing /ck/a? redirector → base64url 解码
            let mut real = href.to_string();
            if real.starts_with("https://www.bing.com/ck/a?")
                && let Ok(parsed) = Url::parse(&real)
                && let Some(u) = parsed
                    .query_pairs()
                    .find(|(k, _)| k == "u")
                    .map(|(_, v)| v.into_owned())
                && let Some(enc) = u.strip_prefix("a1")
            {
                let mut pad = enc.to_string();
                while pad.len() % 4 != 0 {
                    pad.push('=');
                }
                if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE.decode(&pad)
                    && let Ok(s) = String::from_utf8(bytes)
                {
                    real = s;
                }
            }

            let content = item
                .select(&sel_content)
                .next()
                .map(|e| extract_text(&e))
                .unwrap_or_default();

            let published_date = item
                .select(&sel_date)
                .next()
                .and_then(|e| parse_date_text(&extract_text(&e)));

            results.push(RawResult {
                url: real,
                title,
                content,
                published_date,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "bing", count = results.len(), "parsed");
        Ok(results)
    }
}

fn locale_to_mkt(locale: &str) -> &str {
    match locale {
        s if s.starts_with("zh-CN") || s == "zh" => "zh-CN",
        s if s.starts_with("zh") => "zh-HK",
        s if s.starts_with("en-GB") => "en-GB",
        s if s.starts_with("en") => "en-US",
        s if s.starts_with("ja") => "ja-JP",
        s if s.starts_with("ko") => "ko-KR",
        _ => "en-US",
    }
}
