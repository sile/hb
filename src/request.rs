use std::borrow::Cow;
use std::net::SocketAddr;
use fibers::net::TcpStream;
use futures::{Future, Poll, Async, IntoFuture, BoxFuture};
use handy_async::future::Phase;
use miasht;
use miasht::builtin::futures::FutureExt;
use miasht::builtin::futures::WriteAllBytes;
use miasht::builtin::io::IoExt;
use trackable::error::ErrorKindExt;
use url::Url;
use url_serde;

use {Result, Error, ErrorKind};
use run::{Response, Seconds};

pub type TcpConnection = miasht::client::Connection<TcpStream>;
pub type TcpRequest = miasht::client::Request<TcpStream>;
pub type TcpResponse = miasht::client::Response<TcpStream>;

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
        let host = track!(self.url.host_str().ok_or(ErrorKind::Other.error()))?;
        let port = self.url.port_or_known_default().expect("Never fails");
        let addr = format!("{}:{}", host, port);
        track!(addr.parse().map_err(Error::from), "addr={:?}", addr)
    }

    pub fn call(&self, connection: TcpConnection) -> Call {
        use miasht::builtin::headers::ContentLength;
        let mut request = connection.build_request(self.method.into(), &self.path());
        let phase = if let Some(content_size) = self.content {
            request.add_header(&ContentLength(content_size as u64));
            Phase::A(request.finish().write_all_bytes(vec![0; content_size]))
        } else {
            request.add_header(&ContentLength(0));
            Phase::B(request.finish())
        };
        Call { status: 0, phase }
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

pub struct Call {
    status: u16,
    phase: Phase<
        WriteAllBytes<TcpRequest, Vec<u8>>,
        TcpRequest,
        miasht::client::ReadResponse<TcpStream>,
        BoxFuture<(TcpConnection, Vec<u8>), miasht::Error>,
    >,
}
impl Future for Call {
    type Item = (TcpConnection, Response);
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Async::Ready(phase) = track!(self.phase.poll().map_err(Error::from))? {
            let next = match phase {
                Phase::A(request) => Phase::B(request),
                Phase::B(connection) => Phase::C(connection.read_response()),
                Phase::C(response) => {
                    self.status = response.status().code();
                    let future = response
                        .into_body_reader()
                        .into_future()
                        .and_then(|r| r.read_all_bytes())
                        .map(|(r, body)| (r.into_inner().finish(), body))
                        .boxed();
                    Phase::D(future)
                }
                Phase::D((connection, body)) => {
                    let response = Response {
                        status: self.status,
                        content_length: body.len() as u64,
                    };
                    return Ok(Async::Ready((connection, response)));
                }
                _ => unreachable!(),
            };
            self.phase = next;
        }
        Ok(Async::NotReady)
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
impl From<Method> for miasht::Method {
    fn from(f: Method) -> Self {
        match f {
            Method::Put => miasht::Method::Put,
            Method::Post => miasht::Method::Post,
            Method::Get => miasht::Method::Get,
            Method::Head => miasht::Method::Head,
            Method::Delete => miasht::Method::Delete,
        }
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub enum Content {
//     File,
//     Zeros(usize),
// }
pub type Content = usize;
