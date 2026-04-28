use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// searxng 语义等价：main result 的 template 决定分类/去重 key。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ResultTemplate {
    /// 普通网页
    #[default]
    Default,
    /// 视频
    Videos,
    /// 图片
    Images,
    /// 新闻
    News,
}

impl ResultTemplate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Videos => "videos",
            Self::Images => "images",
            Self::News => "news",
        }
    }
}

/// 单个引擎返回的原始结果（未去重、未评分）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawResult {
    pub url: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub template: ResultTemplate,
    pub thumbnail: Option<String>,
    pub img_src: Option<String>,
    pub iframe_src: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<DateTime<Utc>>,
}

impl RawResult {
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            content: content.into(),
            template: ResultTemplate::Default,
            thumbnail: None,
            img_src: None,
            iframe_src: None,
            author: None,
            published_date: None,
        }
    }
}

/// 去重合并后的结果：多个引擎都能命中同一条，会合并到这里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedResult {
    pub url: String,
    pub title: String,
    pub content: String,
    pub template: ResultTemplate,
    pub engines: Vec<String>,
    pub positions: Vec<usize>,
    pub score: f64,
    pub thumbnail: Option<String>,
    pub img_src: Option<String>,
    pub iframe_src: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<DateTime<Utc>>,
}
