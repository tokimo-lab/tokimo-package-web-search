# tokimo-package-web-search

SearXNG-inspired multi-engine web search for Rust — fan-out across search engines, deduplicate, rank, and optionally fetch full article content via Readability.

## Features

- **Multi-engine fan-out** — queries multiple search engines concurrently (`Searcher::search`)
- **Deduplication** — results merged by URL hash (template | host | path | query | fragment | img_src), following SearXNG's `MainResult.__hash__` algorithm
- **Ranking** — scored as `Σ (weight × engines_count) / position`, matching SearXNG's `calculate_score`
- **Detail fetch** — optional per-result Readability denoising via [`tokimo-web-fetch`](https://github.com/tokimo-lab/tokimo-package-web-fetch)
- **Extensible** — implement the `Engine` trait to add custom search engines

## Supported Engines

Google, Bing, DuckDuckGo, Brave, and more — see `src/engines/` for the full list.

## Usage

```rust
use tokimo_web_search::{Searcher, SearchOptions, default_engine_ids};

let searcher = Searcher::new(default_engine_ids());

let results = searcher
    .search(
        "rust async tokio",
        SearchOptions {
            page: 1,
            fetch_detail: false,
            ..Default::default()
        },
    )
    .await?;

for r in &results {
    println!("{} — {}", r.title, r.url);
}
```

## License

MIT
