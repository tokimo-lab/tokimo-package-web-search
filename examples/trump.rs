//! 演示：list 模式 + detail 模式（降噪正文）。
//! cargo run -p tokimo-web-search --example trump -- "特朗普 今天"

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokimo_web_search::{BrowserFetch, LightpandaBrowser, SearchOptions, Searcher};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "特朗普 今天".to_string());
    let out_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "demo.txt".to_string());

    let browser = LightpandaBrowser::autodetect().map(|b| Arc::new(b) as Arc<dyn BrowserFetch>);

    let searcher = Searcher::new_with_browser(&[], browser).expect("build searcher");

    let opts = SearchOptions {
        per_engine_timeout: Duration::from_secs(30),
        max_results: 15,
        ..Default::default()
    };

    println!("[1/2] list mode — 多引擎 fan-out + 去重排序 …");
    let list = searcher.search(&query, &opts).await;

    println!("[2/2] detail mode — 对前 8 条抓 Readability 正文 …");
    let detail_opts = SearchOptions {
        max_results: 8,
        ..opts.clone()
    };
    let detailed = searcher.search_with_details(&query, &detail_opts).await;

    let mut f = File::create(&out_path).expect("create demo.txt");

    writeln!(
        f,
        "================================================================"
    )
    .unwrap();
    writeln!(f, "查询: {query}").unwrap();
    writeln!(
        f,
        "时间: {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%SZ")
    )
    .unwrap();
    writeln!(
        f,
        "================================================================\n"
    )
    .unwrap();

    writeln!(f, "### 引擎执行统计\n").unwrap();
    for s in &list.stats {
        match &s.error {
            None => writeln!(
                f,
                "  {:10}  {:>4}ms  ok    {} items",
                s.engine, s.elapsed_ms, s.count
            )
            .unwrap(),
            Some(e) => {
                writeln!(f, "  {:10}  {:>4}ms  FAIL  {}", s.engine, s.elapsed_ms, e).unwrap()
            }
        }
    }

    writeln!(
        f,
        "\n================================================================"
    )
    .unwrap();
    writeln!(
        f,
        "模式一：列表（去重后 {} 条，title + url + 描述 + 来源引擎）",
        list.results.len()
    )
    .unwrap();
    writeln!(
        f,
        "================================================================\n"
    )
    .unwrap();
    for (i, r) in list.results.iter().enumerate() {
        writeln!(
            f,
            "[{:2}] score={:.3}  engines={:?}",
            i + 1,
            r.score,
            r.engines
        )
        .unwrap();
        writeln!(f, "     title:  {}", r.title).unwrap();
        writeln!(f, "     url:    {}", r.url).unwrap();
        let desc = r.content.chars().take(180).collect::<String>();
        writeln!(f, "     desc:   {desc}\n").unwrap();
    }

    writeln!(
        f,
        "\n================================================================"
    )
    .unwrap();
    writeln!(
        f,
        "模式二：详情（Readability 降噪后的正文，前 {} 条）",
        detailed.results.len()
    )
    .unwrap();
    writeln!(
        f,
        "================================================================\n"
    )
    .unwrap();
    for (i, d) in detailed.results.iter().enumerate() {
        writeln!(
            f,
            "------------------------------------------------------------"
        )
        .unwrap();
        writeln!(f, "[{:2}] {}", i + 1, d.meta.title).unwrap();
        writeln!(f, "     url: {}", d.meta.url).unwrap();
        if let Some(err) = &d.detail_error {
            writeln!(f, "     [detail fetch failed: {err}]").unwrap();
            continue;
        }
        let Some(det) = &d.detail else { continue };
        if let Some(s) = &det.site_name {
            writeln!(f, "     site: {s}").unwrap();
        }
        if let Some(b) = &det.byline {
            writeln!(f, "     byline: {b}").unwrap();
        }
        writeln!(f, "     length: {} chars", det.content_text.chars().count()).unwrap();
        if let Some(ex) = &det.excerpt {
            writeln!(
                f,
                "     excerpt: {}",
                ex.chars().take(200).collect::<String>()
            )
            .unwrap();
        }
        let body = det.content_text.chars().take(1200).collect::<String>();
        writeln!(f, "\n     --- body (前 1200 字) ---\n{body}\n").unwrap();
    }

    println!("\n✅ 已写入 {out_path}");
}
