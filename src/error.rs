use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("captcha / anti-spider triggered on engine `{0}`")]
    Captcha(&'static str),

    #[error("engine `{0}` requires auth (login/cookie) — cannot return results without it")]
    AuthRequired(&'static str),

    #[error("engine `{0}` returned an error: {1}")]
    Engine(&'static str, String),

    #[error("timeout: engine did not respond in time")]
    Timeout,

    #[error("unknown engine: `{0}`")]
    UnknownEngine(String),

    #[error("url error: {0}")]
    Url(#[from] url::ParseError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("readability error: {0}")]
    Readability(String),

    #[error("fetch error: {0}")]
    Fetch(#[from] tokimo_web_fetch::FetchError),
}

pub type SearchResult<T> = std::result::Result<T, SearchError>;
