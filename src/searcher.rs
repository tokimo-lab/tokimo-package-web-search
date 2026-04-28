//! 多引擎调度器：并发 fan-out → 去重合并 → 按 searxng 算法评分排序。
//!
//! 算法严格对齐 `searx/results.py`：
//! - 去重 key = `template | host | path | params | query | fragment | img_src`
//! - 合并：title/content 取更长；engines 并集；positions 追加
//! - 评分：`score = Σ (weight * engines_count) / position`

use crate::engine::{Engine, EngineContext};
use crate::engines::build_engine;
use crate::error::{SearchError, SearchResult};
use crate::readability::{DetailResult, fetch_detail};
use crate::types::{MergedResult, RawResult};
use futures::future::join_all;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use url::Url;

/// 搜索配置
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// 要启用的引擎 ID 列表（默认全部）
    pub engines: Vec<String>,
    pub page: u32,
    pub locale: String,
    pub safesearch: u8,
    /// 单个引擎超时时间
    pub per_engine_timeout: Duration,
    /// 结果最大数量（0 表示不限制）
    pub max_results: usize,
    /// 只搜索国内 / 国外（None 表示全部）
    pub region_filter: Option<RegionFilter>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionFilter {
    ChinaOnly,
    ForeignOnly,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            engines: Vec::new(),
            page: 1,
            locale: "zh-CN".to_string(),
            safesearch: 0,
            per_engine_timeout: Duration::from_secs(5),
            max_results: 50,
            region_filter: None,
        }
    }
}

/// 引擎执行的统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineStat {
    pub engine: String,
    pub count: usize,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<MergedResult>,
    pub stats: Vec<EngineStat>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResponseWithDetails {
    pub query: String,
    pub results: Vec<DetailedResult>,
    pub stats: Vec<EngineStat>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetailedResult {
    #[serde(flatten)]
    pub meta: MergedResult,
    pub detail: Option<DetailResult>,
    pub detail_error: Option<String>,
}

pub struct Searcher {
    client: Client,
    engines: Vec<Arc<dyn Engine>>,
    user_agent: String,
    browser: Option<Arc<dyn crate::browser::BrowserFetch>>,
}

impl Searcher {
    /// 构造一个 Searcher。engine_ids 为空则启用所有可用引擎。
    pub fn new(engine_ids: &[&str]) -> SearchResult<Self> {
        Self::new_with_browser(engine_ids, None)
    }

    /// 同 [`Self::new`] 但附带一个可选的 headless 浏览器；注入后强反爬引擎
    /// （toutiao / zhihu / douyin / google）会优先通过浏览器拉 HTML。
    pub fn new_with_browser(
        engine_ids: &[&str],
        browser: Option<Arc<dyn crate::browser::BrowserFetch>>,
    ) -> SearchResult<Self> {
        let user_agent = default_user_agent();
        let client = Client::builder()
            .user_agent(&user_agent)
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .timeout(Duration::from_secs(15))
            .build()?;

        let engines: Vec<Arc<dyn Engine>> = if engine_ids.is_empty() {
            crate::engines::default_engine_ids()
                .iter()
                .map(|id| build_engine(id).expect("default engine id must be valid"))
                .collect()
        } else {
            engine_ids
                .iter()
                .map(|id| {
                    build_engine(id).ok_or_else(|| SearchError::UnknownEngine((*id).to_string()))
                })
                .collect::<SearchResult<Vec<_>>>()?
        };

        Ok(Self {
            client,
            engines,
            user_agent,
            browser,
        })
    }

    /// 并发搜索所有启用的引擎，返回去重排序后的结果。
    pub async fn search(&self, query: &str, opts: &SearchOptions) -> SearchResponse {
        let selected: Vec<Arc<dyn Engine>> = self
            .engines
            .iter()
            .filter(|e| match opts.region_filter {
                Some(RegionFilter::ChinaOnly) => e.is_china(),
                Some(RegionFilter::ForeignOnly) => !e.is_china(),
                None => true,
            })
            .filter(|e| opts.engines.is_empty() || opts.engines.iter().any(|id| id == e.id()))
            .cloned()
            .collect();

        let ctx = EngineContext {
            client: self.client.clone(),
            query: query.to_string(),
            page: opts.page.max(1),
            locale: opts.locale.clone(),
            safesearch: opts.safesearch,
            user_agent: self.user_agent.clone(),
            browser: self.browser.clone(),
        };

        let timeout = opts.per_engine_timeout;
        let futs = selected.iter().map(|eng| {
            let eng = eng.clone();
            let ctx = ctx.clone();
            async move {
                let start = std::time::Instant::now();
                let res = tokio::time::timeout(timeout, eng.search(&ctx)).await;
                let elapsed_ms = start.elapsed().as_millis() as u64;
                match res {
                    Ok(Ok(items)) => (eng.id(), eng.weight(), Ok(items), elapsed_ms),
                    Ok(Err(e)) => {
                        warn!(engine = eng.id(), error = %e, "engine failed");
                        (eng.id(), eng.weight(), Err(e), elapsed_ms)
                    }
                    Err(_) => {
                        warn!(engine = eng.id(), "engine timed out");
                        (
                            eng.id(),
                            eng.weight(),
                            Err(SearchError::Timeout),
                            elapsed_ms,
                        )
                    }
                }
            }
        });

        let outputs = join_all(futs).await;

        let mut stats = Vec::with_capacity(outputs.len());
        let mut container = ResultContainer::new();
        let mut weights: HashMap<String, f64> = HashMap::new();

        for (id, weight, res, elapsed_ms) in outputs {
            weights.insert(id.to_string(), weight);
            match res {
                Ok(items) => {
                    stats.push(EngineStat {
                        engine: id.to_string(),
                        count: items.len(),
                        elapsed_ms,
                        error: None,
                    });
                    container.extend(id, items);
                }
                Err(e) => {
                    stats.push(EngineStat {
                        engine: id.to_string(),
                        count: 0,
                        elapsed_ms,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let mut results = container.finish(&weights);
        if opts.max_results > 0 && results.len() > opts.max_results {
            results.truncate(opts.max_results);
        }

        SearchResponse {
            query: query.to_string(),
            results,
            stats,
        }
    }

    /// 同 [`search`] 但顺带把每条结果的详情页（Readability 降噪）也抓回来。
    pub async fn search_with_details(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> SearchResponseWithDetails {
        let resp = self.search(query, opts).await;
        let client = self.client.clone();
        let timeout = opts.per_engine_timeout;

        let detail_futs = resp.results.iter().map(|m| {
            let m = m.clone();
            let client = client.clone();
            async move {
                let res = tokio::time::timeout(timeout * 2, fetch_detail(&client, &m.url)).await;
                let (detail, err) = match res {
                    Ok(Ok(d)) => (Some(d), None),
                    Ok(Err(e)) => (None, Some(e.to_string())),
                    Err(_) => (None, Some("timeout".to_string())),
                };
                DetailedResult {
                    meta: m,
                    detail,
                    detail_error: err,
                }
            }
        });

        let results = join_all(detail_futs).await;

        SearchResponseWithDetails {
            query: query.to_string(),
            results,
            stats: resp.stats,
        }
    }
}

// ─── 去重 / 合并 / 评分 ────────────────────────────────────────────────────

/// 参考 searxng `MainResult.__hash__`：用 template + URL 关键分量 + img_src 做 key。
fn dedup_key(r: &RawResult) -> String {
    let parsed = Url::parse(&r.url).ok();
    let (host, path, query, fragment) = parsed.as_ref().map_or_else(
        || (String::new(), r.url.clone(), String::new(), String::new()),
        |u| {
            (
                u.host_str().unwrap_or("").to_string(),
                u.path().to_string(),
                u.query().unwrap_or("").to_string(),
                u.fragment().unwrap_or("").to_string(),
            )
        },
    );

    format!(
        "{}|{}|{}|{}|{}|{}",
        r.template.as_str(),
        strip_www(&host),
        path,
        query,
        fragment,
        r.img_src.as_deref().unwrap_or("")
    )
}

fn strip_www(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

struct ResultContainer {
    /// key → (result, engines, positions)
    map: HashMap<String, Bucket>,
    /// 稳定顺序
    order: Vec<String>,
}

struct Bucket {
    result: RawResult,
    engines: Vec<String>,
    positions: Vec<usize>,
}

impl ResultContainer {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn extend(&mut self, engine: &str, items: Vec<RawResult>) {
        for (idx, item) in items.into_iter().enumerate() {
            if item.url.is_empty() || item.title.is_empty() {
                continue;
            }
            let pos = idx + 1;
            let key = dedup_key(&item);

            if let Some(bucket) = self.map.get_mut(&key) {
                // 合并：title/content 取更长；url 取 https 优先；engines 并集；positions 追加
                merge_in_place(&mut bucket.result, item);
                if !bucket.engines.iter().any(|e| e == engine) {
                    bucket.engines.push(engine.to_string());
                }
                bucket.positions.push(pos);
            } else {
                self.order.push(key.clone());
                self.map.insert(
                    key,
                    Bucket {
                        result: item,
                        engines: vec![engine.to_string()],
                        positions: vec![pos],
                    },
                );
            }
        }
    }

    fn finish(self, weights: &HashMap<String, f64>) -> Vec<MergedResult> {
        let Self { mut map, order } = self;
        let mut merged: Vec<MergedResult> = Vec::with_capacity(order.len());

        for key in order {
            if let Some(b) = map.remove(&key) {
                let score = calculate_score(&b.engines, &b.positions, weights);
                merged.push(MergedResult {
                    url: b.result.url,
                    title: b.result.title,
                    content: b.result.content,
                    template: b.result.template,
                    engines: b.engines,
                    positions: b.positions,
                    score,
                    thumbnail: b.result.thumbnail,
                    img_src: b.result.img_src,
                    iframe_src: b.result.iframe_src,
                    author: b.result.author,
                    published_date: b.result.published_date,
                });
            }
        }

        // 按分数降序
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        debug!(total = merged.len(), "dedup merged");
        merged
    }
}

fn merge_in_place(origin: &mut RawResult, other: RawResult) {
    if other.content.len() > origin.content.len() {
        origin.content = other.content;
    }
    if other.title.len() > origin.title.len() {
        origin.title = other.title;
    }
    if origin.thumbnail.is_none() && other.thumbnail.is_some() {
        origin.thumbnail = other.thumbnail;
    }
    if origin.img_src.is_none() && other.img_src.is_some() {
        origin.img_src = other.img_src;
    }
    if origin.iframe_src.is_none() && other.iframe_src.is_some() {
        origin.iframe_src = other.iframe_src;
    }
    if origin.author.is_none() && other.author.is_some() {
        origin.author = other.author;
    }
    if origin.published_date.is_none() && other.published_date.is_some() {
        origin.published_date = other.published_date;
    }
    // 优先 https
    if origin.url.starts_with("http://") && other.url.starts_with("https://") {
        origin.url = other.url;
    }
}

/// 严格对齐 searxng `results.py::calculate_score`：
/// `weight = Π engine.weight`（命中此结果的所有引擎），再乘 `len(positions)`；
/// `score = Σ weight / position`
fn calculate_score(engines: &[String], positions: &[usize], weights: &HashMap<String, f64>) -> f64 {
    let mut weight = 1.0f64;
    for e in engines {
        if let Some(w) = weights.get(e) {
            weight *= *w;
        }
    }
    weight *= positions.len() as f64;

    let mut score = 0.0;
    for &pos in positions {
        score += weight / pos as f64;
    }
    score
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:135.0) Gecko/20100101 Firefox/135.0".to_string()
}
