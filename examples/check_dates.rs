//! 检查各引擎是否返回 published_date。

#![allow(clippy::print_stdout, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use tokimo_web_fetch::autodetect_browser;
use tokimo_web_search::{SearchOptions, Searcher, available_engines};

#[tokio::main]
async fn main() {
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rust programming language".to_string());
    println!("Query: {query}\n");

    let browser = autodetect_browser();
    if browser.is_some() {
        println!("Headless browser: Chrome detected\n");
    } else {
        println!("Headless browser: NOT found (Google/Toutiao/Douyin will use HTTP fallback)\n");
    }

    let engine_ids: Vec<&str> = vec![
        "google",
        "bing",
        "yahoo",
        "duckduckgo",
        "baidu",
        "bilibili",
        "sogou",
        "360",
        "chinaso",
        "github",
        "github-code",
        "csdn",
        "juejin",
    ];

    for eid in &engine_ids {
        let s = match Searcher::new_with_browser(&[eid], browser.clone()) {
            Ok(s) => s,
            Err(e) => {
                println!("[{eid}] build failed: {e}");
                continue;
            }
        };
        let opts = SearchOptions {
            engines: vec![],
            page: 1,
            locale: "en-US".to_string(),
            safesearch: 0,
            per_engine_timeout: Duration::from_secs(15),
            max_results: 5,
            region_filter: None,
        };
        let resp = s.search(&query, &opts).await;
        let results = &resp.results;
        let with_date = results
            .iter()
            .filter(|r| r.published_date.is_some())
            .count();
        let total = results.len();

        if let Some(err) = resp.stats.iter().find_map(|s| s.error.as_deref()) {
            println!("[{eid}] FAIL: {err}");
            continue;
        }

        println!("[{eid}] {total} results, {with_date} with date");

        for r in results.iter().take(3) {
            let date_str = match &r.published_date {
                Some(d) => d.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                None => "None".to_string(),
            };
            let title: String = r.title.chars().take(60).collect();
            println!("  date={date_str}  title={title}");
        }
    }

    let all = available_engines();
    println!("\nRegistered engines ({}):", all.len());
    for id in all {
        println!("  {id}");
    }
}
