//! 浏览器抽象 / Lightpanda 实现已迁移到 `tokimo-web-fetch`。
//!
//! 本模块只做 re-export 保持向后兼容。

pub use tokimo_web_fetch::{BrowserFetch, LightpandaBrowser};
