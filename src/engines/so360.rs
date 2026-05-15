//! 360 搜索 — 参考 searxng `engines/360search.py`
//!
//! URL: https://www.so.com/s?pn=&q=
//! 先做一次预请求拿 set-cookie，再带 cookie 重新请求。

use crate::engine::{Engine, EngineContext};
use crate::engines::common::{extract_text, parse_date_text};
use crate::error::SearchResult;
use crate::sel;
use crate::types::RawResult;
use async_trait::async_trait;
use scraper::Html;
use tokio::sync::OnceCell;
use tracing::debug;

pub struct So360;

static COOKIE: OnceCell<Option<String>> = OnceCell::const_new();

#[async_trait]
impl Engine for So360 {
    fn id(&self) -> &'static str {
        "360"
    }
    fn warmup_url(&self) -> Option<&str> {
        Some("https://www.so.com/")
    }
    fn is_china(&self) -> bool {
        true
    }

    async fn search(&self, ctx: &EngineContext) -> SearchResult<Vec<RawResult>> {
        let page = ctx.page.max(1);
        let q = urlencoding::encode(&ctx.query);
        let url = format!("https://www.so.com/s?pn={page}&q={q}");

        let cookie = COOKIE
            .get_or_init(|| async {
                let r = ctx.client.get(&url).send().await.ok()?;
                let v = r.headers().get("set-cookie")?.to_str().ok()?;
                Some(v.split(';').next().unwrap_or_default().to_string())
            })
            .await
            .clone();

        let mut req = ctx.client.get(&url);
        if let Some(c) = cookie {
            req = req.header("Cookie", c);
        }
        let body = req.send().await?.text().await?;
        let doc = Html::parse_document(&body);

        let sel_item = sel!(r"li.res-list");
        let sel_title = sel!(r"h3.res-title a");
        let sel_desc1 = sel!(r"p.res-desc");
        let sel_desc2 = sel!(r"span.res-list-summary");
        let sel_date = sel!(r"span.gray");

        let mut out = Vec::new();
        for item in doc.select(&sel_item) {
            let Some(a) = item.select(&sel_title).next() else {
                continue;
            };
            let title = extract_text(&a);
            let url = a
                .value()
                .attr("data-mdurl")
                .or_else(|| a.value().attr("href"))
                .unwrap_or_default()
                .to_string();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let content = item
                .select(&sel_desc1)
                .next()
                .or_else(|| item.select(&sel_desc2).next())
                .map(|e| extract_text(&e))
                .unwrap_or_default();
            let published_date = item
                .select(&sel_date)
                .next()
                .and_then(|e| parse_date_text(&extract_text(&e)));
            out.push(RawResult {
                url,
                title,
                content,
                published_date,
                ..RawResult::new("", "", "")
            });
        }

        debug!(engine = "360", count = out.len(), "parsed");
        Ok(out)
    }
}
