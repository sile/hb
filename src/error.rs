use fibers::sync::oneshot::MonitorError;
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::mpsc::{RecvError, SendError};
use std::sync::PoisonError;
use trackable::error::TrackableError;
use trackable::error::{ErrorKind as TrackableErrorKind, ErrorKindExt};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorKind {
    Timeout,
    Other,
}
impl TrackableErrorKind for ErrorKind {}

#[derive(Debug, Clone, Serialize, Deserialize, trackable::TrackableError)]
pub struct Error(TrackableError<ErrorKind>);
impl From<io::Error> for Error {
    fn from(f: io::Error) -> Self {
        ErrorKind::Other.cause(f).into()
    }
}
impl From<std::net::AddrParseError> for Error {
    fn from(f: std::net::AddrParseError) -> Self {
        ErrorKind::Other.cause(f).into()
    }
}
impl From<fibers_http_client::Error> for Error {
    fn from(f: fibers_http_client::Error) -> Self {
        let original_error_kind = *f.kind();
        let kind = match original_error_kind {
            fibers_http_client::ErrorKind::Timeout => ErrorKind::Timeout,
            _ => ErrorKind::Other,
        };
        track!(kind.takes_over(f); original_error_kind).into()
    }
}
impl From<serdeconv::Error> for Error {
    fn from(f: serdeconv::Error) -> Self {
        ErrorKind::Other.takes_over(f).into()
    }
}
impl From<MonitorError<Error>> for Error {
    fn from(f: MonitorError<Error>) -> Self {
        f.unwrap_or_else(|| {
            ErrorKind::Other
                .cause("Monitoring channel disconnected")
                .into()
        })
    }
}
impl<T> From<SendError<T>> for Error {
    fn from(f: SendError<T>) -> Self {
        ErrorKind::Other.cause(f.to_string()).into()
    }
}
impl From<RecvError> for Error {
    fn from(f: RecvError) -> Self {
        ErrorKind::Other.cause(f).into()
    }
}
impl<T> From<PoisonError<T>> for Error {
    fn from(f: PoisonError<T>) -> Self {
        ErrorKind::Other.cause(f.to_string()).into()
    }
}
