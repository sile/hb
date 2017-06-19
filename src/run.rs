use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use fibers::Spawn;
use futures::{Future, Poll};
use serdeconv;

use {Result, Error};
use request::Request;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub seq_no: usize,
    pub status: u16,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct RequestQueue {
    requests: Arc<Mutex<VecDeque<Request>>>,
}
impl RequestQueue {
    pub fn read_from<R: Read>(reader: R) -> Result<Self> {
        let requests = track!(serdeconv::from_json_reader(reader))?;
        let requests = Arc::new(Mutex::new(requests));
        Ok(RequestQueue { requests })
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionPool;

#[derive(Debug)]
pub struct ClientFiber {}

#[derive(Debug)]
pub struct RunnerBuilder {
    concurrency: usize,
}
impl RunnerBuilder {
    pub fn new() -> Self {
        RunnerBuilder::default()
    }
    pub fn finish<S>(&self, spawner: S, requests: RequestQueue) -> Runner
    where
        S: Spawn,
    {
        let responses = Vec::with_capacity(requests.requests.lock().unwrap().len());
        let pool = ConnectionPool;
        for _ in 0..self.concurrency {
            let future = ClientFiber::new(pool.clone(), requests.clone());
            spawner.spawn(future);
        }
        Runner { responses }
    }
}
impl Default for RunnerBuilder {
    fn default() -> Self {
        RunnerBuilder { concurrency: 32 }
    }
}

#[derive(Debug)]
pub struct Runner {
    responses: Vec<Response>,
}
impl Runner {
    pub fn new<S: Spawn>(spawner: S, requests: RequestQueue) -> Self {
        RunnerBuilder::new().finish(spawner, requests)
    }
}
impl Future for Runner {
    type Item = Vec<Response>;
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        panic!()
    }
}
