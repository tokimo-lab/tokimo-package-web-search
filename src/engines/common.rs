//! 引擎之间共享的小工具：HTML 文本抽取 / URL 规范化 等。

use scraper::{ElementRef, Selector};

/// 仿 searxng `extract_text`：把一个节点下所有可见文本收集成一行，压掉连续空白。
pub fn extract_text(el: &ElementRef<'_>) -> String {
    let raw: String = el.text().collect::<Vec<_>>().join(" ");
    collapse_whitespace(&raw)
}

#[allow(dead_code)]
pub fn extract_text_from_sel(root: &ElementRef<'_>, sel: &Selector) -> String {
    root.select(sel).next().map(|e| extract_text(&e)).unwrap_or_default()
}

pub fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// 去掉 HTML 实体（&amp; 等）
pub fn html_unescape(s: &str) -> String {
    html_escape::decode_html_entities(s).into_owned()
}

/// 把裸 HTML 字符串压成纯文本
pub fn html_to_text(s: &str) -> String {
    if !s.contains('<') {
        return collapse_whitespace(&html_unescape(s));
    }
    // 简单去 tag：scraper 需要完整 document，加 wrapper
    let wrapped = format!("<div>{s}</div>");
    let frag = scraper::Html::parse_fragment(&wrapped);
    let root = frag.root_element();
    extract_text(&root)
}

/// 组合 selector 构造快捷宏
#[macro_export]
macro_rules! sel {
    ($e:expr) => {
        ::scraper::Selector::parse($e).expect("invalid selector")
    };
}
