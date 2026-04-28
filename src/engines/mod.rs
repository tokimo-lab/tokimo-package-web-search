//! 所有引擎实现及注册表。

mod baidu;
mod bilibili;
mod bing;
mod chinaso;
mod common;
mod douyin;
mod google;
mod so360;
mod sogou;
mod toutiao;
mod yahoo;
mod zhihu;

use crate::engine::Engine;
use std::sync::Arc;

/// 所有已实现引擎 ID（稳定顺序）
pub fn available_engines() -> &'static [&'static str] {
    &[
        "google", "bing", "yahoo", "baidu", "bilibili", "sogou", "360", "chinaso", "zhihu", "toutiao", "douyin",
    ]
}

/// 默认启用的引擎：只包含"开箱可用"的（Google 依用户要求默认开启，
/// 实际是否出结果取决于出口 IP 是否被 reCAPTCHA 拦截）。
/// 排除：`chinaso`（SSR 空占位）、`zhihu`（zse-ck 反爬）、`douyin`（VM 混淆）。
/// `toutiao` 在注入 headless browser 时才会返回结果，未注入时会抛 AuthRequired。
pub fn default_engine_ids() -> &'static [&'static str] {
    &[
        "google", "bing", "yahoo", "baidu", "bilibili", "sogou", "360", "toutiao",
    ]
}

/// 按 ID 构造引擎实例
pub fn build_engine(id: &str) -> Option<Arc<dyn Engine>> {
    let e: Arc<dyn Engine> = match id {
        "google" => Arc::new(google::Google),
        "bing" => Arc::new(bing::Bing),
        "yahoo" => Arc::new(yahoo::Yahoo),
        "baidu" => Arc::new(baidu::Baidu),
        "bilibili" => Arc::new(bilibili::Bilibili),
        "sogou" => Arc::new(sogou::Sogou),
        "360" | "so360" => Arc::new(so360::So360),
        "chinaso" => Arc::new(chinaso::ChinaSo),
        "zhihu" => Arc::new(zhihu::Zhihu),
        "toutiao" => Arc::new(toutiao::Toutiao),
        "douyin" => Arc::new(douyin::Douyin),
        _ => return None,
    };
    Some(e)
}
