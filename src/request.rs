use url::Url;
use url_serde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub method: Method,
    #[serde(with = "url_serde")]
    pub url: Url,
    pub content: Option<Content>,
    pub timeout: Option<u32>,
    // thread, time, header
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Put,
    Post,
    Get,
    Head,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Content {
    File,
    Zeros(usize),
}
