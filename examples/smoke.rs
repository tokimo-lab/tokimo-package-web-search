//! 端到端冒烟测试：跑所有引擎，打印结果统计。
//!
//! 运行：`cargo run -p tokimo-web-search --example smoke -- "关键词"`

#![allow(clippy::print_stdout, clippy::unwrap_used)]

use std::time::Duration;

use tokimo_web_search::{SearchOptions, Searcher};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rust async".to_string());
    println!("Searching: {query}\n");

    // 如果系统里有 Chrome，就启用 headless 路径
    let browser = tokimo_web_fetch::ChromeBrowser::autodetect();
    if browser.is_some() {
        println!("(chrome detected — toutiao/etc will use headless)\n");
    }
    let browser = browser
        .map(|b| std::sync::Arc::new(b) as std::sync::Arc<dyn tokimo_web_fetch::BrowserFetch>);

    let searcher = Searcher::new_with_browser(&[], browser).expect("build searcher");
    let opts = SearchOptions {
        engines: vec![],
        page: 1,
        locale: "en-US".to_string(),
        safesearch: 0,
        per_engine_timeout: Duration::from_secs(30),
        max_results: 20,
        region_filter: None,
    };

    let resp = searcher.search(&query, &opts).await;

    println!("=== Engine stats ===");
    for s in &resp.stats {
        match &s.error {
            Some(e) => println!("  {:12} {:>4}ms  FAIL  {e}", s.engine, s.elapsed_ms),
            None => println!(
                "  {:12} {:>4}ms  ok    {} items",
                s.engine, s.elapsed_ms, s.count
            ),
        }
    }

    println!("\n=== Top {} merged ===", resp.results.len());
    for (i, r) in resp.results.iter().take(20).enumerate() {
        println!(
            "{:2}. [score={:.3}] {} [{}]\n    {}\n    {}",
            i + 1,
            r.score,
            r.title,
            r.engines.join(","),
            r.url,
            truncate(&r.content, 120)
        );
    }

    if let Some(top) = resp.results.iter().find(|r| r.url.starts_with("http")) {
        println!("\n=== Readability detail for top result ===");
        println!("URL: {}", top.url);
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        match tokio::time::timeout(
            Duration::from_secs(20),
            tokimo_web_search::fetch_detail(&client, &top.url),
        )
        .await
        {
            Ok(Ok(d)) => {
                println!("title   : {}", d.title);
                println!("byline  : {:?}", d.byline);
                println!("length  : {}", d.length);
                println!(
                    "excerpt : {}",
                    truncate(d.excerpt.as_deref().unwrap_or(""), 160)
                );
                println!("text(head): {}", truncate(&d.content_text, 300));
            }
            Ok(Err(e)) => println!("detail error: {e}"),
            Err(_) => println!("detail timeout"),
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n).collect();
        format!("{cut}…")
    }
}
