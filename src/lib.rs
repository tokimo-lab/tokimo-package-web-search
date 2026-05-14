//! tokimo-web-search — SearXNG-inspired multi-engine web search.
//!
//! 设计思路 1:1 参考 searxng：
//! - 多引擎并发 fan-out（[`Searcher::search`]）
//! - 每个引擎返回 [`RawResult`]，结果按「模板 | host | path | query | fragment | img_src」
//!   做 hash 去重（参考 `searx/result_types/_base.py` `MainResult.__hash__`）
//! - 重复结果合并：title/content 取更长、engines 取并集、positions 累加
//! - 排序：`score = Σ (weight * engines_count) / position`（参考 `searx/results.py` `calculate_score`）
//! - 详情抓取：Readability（`dom_smoothie`），用来做 HTML 降噪

#![allow(clippy::module_name_repetitions)]

pub mod engine;
pub mod engines;
pub mod error;
pub mod readability;
pub mod searcher;
pub mod types;

pub use engine::{Engine, EngineContext};
pub use engines::{available_engines, build_engine, default_engine_ids};
pub use error::{SearchError, SearchResult as Result};
pub use readability::{DetailResult, fetch_detail};
pub use searcher::{EngineStat, SearchOptions, Searcher};
pub use types::{MergedResult, RawResult, ResultTemplate};
