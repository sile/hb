use fibers_http_client::connection::ConnectionPoolHandle;
use fibers_http_client::Client;
use futures::Future;
use httpcodec::Response;
use std::borrow::Cow;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use trackable::error::ErrorKindExt;
use url::Url;
use url_serde;

use run::Seconds;
use {Error, ErrorKind, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub method: Method,
    #[serde(with = "url_serde")]
    pub url: Url,
    pub content: Option<Content>,
    pub timeout: Option<Seconds>,
    pub start_time: Option<Seconds>,
    // thread, time, header
}
impl Request {
    pub fn addr(&self) -> Result<SocketAddr> {
        // TODO: check scheme
        let host = track!(self.url.host_str().ok_or_else(|| ErrorKind::Other.error()))?;
        let port = self.url.port_or_known_default().expect("Never fails");
        let mut addrs = track!(
            (host, port).to_socket_addrs().map_err(Error::from),
            "{}:{}",
            host,
            port
        )?;
        let addr = track_assert_some!(addrs.next(), ErrorKind::Other);
        Ok(addr)
    }

    pub fn call(
        &self,
        client: &mut Client<ConnectionPoolHandle>,
        timeout: Option<Duration>,
    ) -> Box<Future<Item = Response<Vec<u8>>, Error = Error> + Send + 'static> {
        let mut request = client.request(&self.url);
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        match self.method {
            Method::Get => Box::new(request.get().map_err(Error::from)),
            Method::Head => Box::new(
                request
                    .head()
                    .map(|res| res.map_body(|_| Vec::new()))
                    .map_err(Error::from),
            ),
            Method::Delete => Box::new(request.delete().map_err(Error::from)),
            Method::Post => {
                let content = self.content.as_ref().map_or(Vec::new(), |c| c.to_bytes());
                Box::new(request.post(content).map_err(Error::from))
            }
            Method::Put => {
                let content = self.content.as_ref().map_or(Vec::new(), |c| c.to_bytes());
                Box::new(request.put(content).map_err(Error::from))
            }
        }
    }

    pub fn path(&self) -> Cow<str> {
        if self.url.query().is_none() && self.url.fragment().is_none() {
            Cow::Borrowed(self.url.path())
        } else {
            let mut path = self.url.path().to_string();
            if let Some(query) = self.url.query() {
                path.push('?');
                path.push_str(query);
            }
            if let Some(fragment) = self.url.fragment() {
                path.push('#');
                path.push_str(fragment);
            }
            Cow::Owned(path)
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Put,
    Post,
    Get,
    Head,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Size(usize),
    Text(String),
}
impl Content {
    pub fn size(&self) -> usize {
        match *self {
            Content::Size(size) => size,
            Content::Text(ref text) => text.len(),
        }
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        match *self {
            Content::Size(size) => vec![0; size],
            Content::Text(ref text) => text.clone().into_bytes(),
        }
    }
}
