use std::collections::BinaryHeap;
use std::io::Read;
use std::mem;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{self, Duration};
use slog::Logger;
use fibers::Spawn;
use fibers::sync::mpsc;
use fibers::time::timer;
use futures::{Future, Poll, Async, Stream};
use handy_async::future::Phase;
use serdeconv;

use {Result, Error, ErrorKind};
use request::{self, Request};
use connection_pool::{self, ConnectionPool, ConnectionPoolHandle};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Seconds(pub f64);
impl From<Seconds> for Duration {
    fn from(f: Seconds) -> Self {
        Duration::new(f.0 as u64, (f.0.fract() * 1_000_000_000.0) as u32)
    }
}
impl From<Duration> for Seconds {
    fn from(f: Duration) -> Self {
        Seconds(
            f.as_secs() as f64 + (f.subsec_nanos() as f64 / 1_000_000_000.0),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "result")]
pub enum RequestResult {
    Ok {
        seq_no: usize,
        end_time: Seconds,
        elapsed: Seconds,
        response: Response,
    },
    Error {
        seq_no: usize,
        end_time: Seconds,
        elapsed: Seconds,
        error: ErrorKind,
    },
}
impl RequestResult {
    pub fn seq_no(&self) -> usize {
        match *self {
            RequestResult::Ok { seq_no, .. } => seq_no,
            RequestResult::Error { seq_no, .. } => seq_no,
        }
    }
    pub fn elapsed(&self) -> Seconds {
        match *self {
            RequestResult::Ok { elapsed, .. } => elapsed,
            RequestResult::Error { elapsed, .. } => elapsed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub content_length: u64,
}

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub seq_no: usize,
    pub request: Request,
}
impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.request.start_time == other.request.start_time
    }
}
impl Eq for QueueItem {}
impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        (other.request.start_time, other.seq_no).partial_cmp(&(
            self.request
                .start_time,
            self.seq_no,
        ))
    }
}
impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct RequestQueue {
    requests: Arc<Mutex<BinaryHeap<QueueItem>>>,
}
impl RequestQueue {
    pub fn read_from<R: Read>(reader: R) -> Result<Self> {
        let requests: Vec<Request> = track!(serdeconv::from_json_reader(reader))?;
        let requests = Arc::new(Mutex::new(
            requests
                .into_iter()
                .enumerate()
                .map(|(seq_no, request)| QueueItem { seq_no, request })
                .collect(),
        ));
        Ok(RequestQueue { requests })
    }
    pub fn push(&self, seq_no: usize, request: Request) -> Result<()> {
        let mut requests = track!(self.requests.lock().map_err(Error::from))?;
        requests.push(QueueItem { seq_no, request });
        Ok(())
    }
    pub fn pop(&self) -> Result<Option<(usize, Request)>> {
        let mut requests = track!(self.requests.lock().map_err(Error::from))?;
        Ok(requests.pop().map(|x| (x.seq_no, x.request)))
    }
}

pub struct RunRequest {
    timeout: Option<timer::Timeout>,
    request: Request,
    addr: SocketAddr,
    phase: Phase<connection_pool::AcquireConnection, request::Call>,
}
impl RunRequest {
    pub fn new(request: Request, pool: &ConnectionPoolHandle) -> Result<Self> {
        let timeout = request.timeout.map(|d| timer::timeout(d.into()));
        let addr = track!(request.addr())?;
        let phase = Phase::A(pool.acquire_connection(addr));
        Ok(RunRequest {
            timeout,
            request,
            addr,
            phase,
        })
    }
}
impl Future for RunRequest {
    type Item = (SocketAddr, request::TcpConnection, Response);
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(Some(())) = track!(self.timeout.poll().map_err(Error::from))? {
            track_panic!(ErrorKind::Timeout);
        }
        while let Async::Ready(phase) = track!(self.phase.poll().map_err(Error::from))? {
            let next = match phase {
                Phase::A(connection) => {
                    let future = self.request.call(connection);
                    Phase::B(future)
                }
                Phase::B((connection, response)) => {
                    return Ok(Async::Ready((self.addr, connection, response)))
                }
                _ => unreachable!(),
            };
            self.phase = next;
        }
        Ok(Async::NotReady)
    }
}

pub struct ClientFiber {
    logger: Logger,
    pool: ConnectionPoolHandle,
    requests: RequestQueue,
    response_tx: mpsc::Sender<RequestResult>,
    last_seq_no: usize, // TODO
    start_time: time::Instant,
    bench_start: time::Instant,
    next_start: Option<timer::Timeout>,
    future: Option<RunRequest>,
}
impl ClientFiber {
    pub fn new(
        logger: Logger,
        pool: ConnectionPoolHandle,
        bench_start: time::Instant,
        requests: RequestQueue,
        response_tx: mpsc::Sender<RequestResult>,
    ) -> Self {
        info!(logger, "Starts a client");
        ClientFiber {
            logger,
            pool,
            last_seq_no: 0,
            start_time: time::Instant::now(),
            bench_start,
            requests,
            response_tx,
            next_start: None,
            future: None,
        }
    }
}
impl Future for ClientFiber {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // TODO: handle error
        loop {
            if let Async::NotReady = self.next_start.poll().unwrap() {
                return Ok(Async::NotReady);
            }
            self.next_start = None;

            match self.future.poll() {
                Err(e) => {
                    let result = RequestResult::Error {
                        seq_no: self.last_seq_no,
                        end_time: self.bench_start.elapsed().into(),
                        elapsed: self.start_time.elapsed().into(),
                        error: *e.kind(),
                    };
                    info!(
                        self.logger,
                        "Failed to request: seq_no={}, error={:?}, elapsed={}",
                        result.seq_no(),
                        e.kind(),
                        result.elapsed().0
                    );
                    debug!(self.logger, "{}", e);
                    track!(self.response_tx.send(result).map_err(Error::from))?;
                    self.future = None;
                }
                Ok(Async::Ready(Some((addr, connection, response)))) => {
                    let result = RequestResult::Ok {
                        seq_no: self.last_seq_no,
                        end_time: self.bench_start.elapsed().into(),
                        elapsed: self.start_time.elapsed().into(),
                        response,
                    };
                    info!(
                        self.logger,
                        "Succeeded to request: seq_no={}, elapsed={}",
                        result.seq_no(),
                        result.elapsed().0
                    );
                    track!(self.response_tx.send(result).map_err(Error::from))?;
                    self.pool.release_connection(addr, connection);
                    self.future = None;
                }
                Ok(Async::Ready(None)) => {
                    if let Some((seq_no, request)) = track!(self.requests.pop())? {
                        if let Some(start_time) = request.start_time {
                            let elapsed = self.bench_start.elapsed();
                            let start_time = Duration::from(start_time);
                            if elapsed <= start_time {
                                let wait = start_time - elapsed;
                                info!(self.logger, "Wait: {:?}", wait);
                                self.next_start = Some(timer::timeout(wait));
                                track!(self.requests.push(seq_no, request))?;
                                continue;
                            }
                        }

                        info!(self.logger, "New request is started: seq_no={}", seq_no);
                        self.last_seq_no = seq_no;
                        self.start_time = time::Instant::now();
                        let future = track!(RunRequest::new(request, &self.pool))?;
                        self.future = Some(future);
                    } else {
                        return Ok(Async::Ready(()));
                    }
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            }
        }
    }
}

#[derive(Debug)]
pub struct RunnerBuilder {
    concurrency: usize,
}
impl RunnerBuilder {
    pub fn new() -> Self {
        RunnerBuilder::default()
    }
    pub fn concurrency(&mut self, concurrency: usize) -> &mut Self {
        self.concurrency = concurrency;
        self
    }
    pub fn finish<S>(&self, logger: Logger, spawner: S, requests: RequestQueue) -> Runner
    where
        S: Spawn,
    {
        let bench_start = time::Instant::now();
        let responses = Vec::with_capacity(requests.requests.lock().unwrap().len());
        let pool = ConnectionPool::new();
        let (response_tx, response_rx) = mpsc::channel();
        for i in 0..self.concurrency {
            let logger = logger.new(o!("id" => i));
            let future = ClientFiber::new(
                logger,
                pool.handle(),
                bench_start,
                requests.clone(),
                response_tx.clone(),
            );
            spawner.spawn(future.map_err(|e| panic!("Error: {}", e)));
        }
        spawner.spawn(pool.map_err(|e| panic!("Error: {}", e)));
        Runner {
            logger,
            responses,
            response_rx,
        }
    }
}
impl Default for RunnerBuilder {
    fn default() -> Self {
        RunnerBuilder { concurrency: 128 }
    }
}

#[derive(Debug)]
pub struct Runner {
    logger: Logger,
    responses: Vec<RequestResult>,
    response_rx: mpsc::Receiver<RequestResult>,
}
impl Runner {
    pub fn new<S: Spawn>(logger: Logger, spawner: S, requests: RequestQueue) -> Self {
        RunnerBuilder::new().finish(logger, spawner, requests)
    }
}
impl Future for Runner {
    type Item = Vec<RequestResult>;
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Async::Ready(polled) = self.response_rx.poll().expect("Never fails") {
            if let Some(response) = polled {
                self.responses.push(response);
                if self.responses.len() == self.responses.capacity() {
                    let mut responses = mem::replace(&mut self.responses, Vec::new());
                    responses.sort_by_key(|r| r.seq_no());
                    return Ok(Async::Ready(responses));
                }
            } else {
                track_panic!(ErrorKind::Other, "All workers down");
            }
        }
        Ok(Async::NotReady)
    }
}
