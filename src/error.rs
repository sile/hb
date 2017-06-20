use std;
use std::io;
use std::sync::PoisonError;
use std::sync::mpsc::{SendError, RecvError};
use fibers::sync::oneshot::MonitorError;
use handy_async::future::Phase;
use miasht;
use serdeconv;
use trackable::error::TrackableError;
use trackable::error::{ErrorKind as TrackableErrorKind, ErrorKindExt};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorKind {
    Timeout,
    Other,
}
impl TrackableErrorKind for ErrorKind {}

#[derive(Debug, Clone)]
pub struct Error(TrackableError<ErrorKind>);
derive_traits_for_trackable_error_newtype!(Error, ErrorKind);
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
impl From<miasht::Error> for Error {
    fn from(f: miasht::Error) -> Self {
        ErrorKind::Other.takes_over(f).into()
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
impl<A, B, C, D, E> From<Phase<A, B, C, D, E>> for Error
where
    Error: From<A>,
    Error: From<B>,
    Error: From<C>,
    Error: From<D>,
    Error: From<E>,
{
    fn from(f: Phase<A, B, C, D, E>) -> Self {
        match f {
            Phase::A(e) => track!(Error::from(e), "Phase::A"),
            Phase::B(e) => track!(Error::from(e), "Phase::B"),
            Phase::C(e) => track!(Error::from(e), "Phase::C"),
            Phase::D(e) => track!(Error::from(e), "Phase::D"),
            Phase::E(e) => track!(Error::from(e), "Phase::E"),
        }
    }
}
