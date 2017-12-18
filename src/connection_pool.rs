use std::collections::HashMap;
use std::net::SocketAddr;
use fibers::net::TcpStream;
use fibers::sync::mpsc;
use fibers::sync::oneshot;
use futures::{self, Async, Future, Poll, Stream};
use futures::future::{Failed, Finished};
use handy_async::future::Phase;
use miasht;

use Error;

pub type TcpConnection = miasht::client::Connection<TcpStream>;

#[derive(Debug)]
pub enum Command {
    AcquireConnection {
        addr: SocketAddr,
        reply: oneshot::Sender<FutureConnection>,
    },
    ReleaseConnection {
        addr: SocketAddr,
        connection: TcpConnection,
    },
}

#[derive(Debug)]
pub struct FutureConnection {
    phase: Phase<Finished<TcpConnection, Error>, miasht::client::Connect>,
}
impl FutureConnection {
    fn from_connection(connection: TcpConnection) -> Self {
        let phase = Phase::A(futures::finished(connection));
        FutureConnection { phase }
    }
    fn connect(addr: SocketAddr) -> Self {
        let phase = Phase::B(miasht::client::Client::new().connect(addr));
        FutureConnection { phase }
    }
}
impl Future for FutureConnection {
    type Item = TcpConnection;
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(phase) = track!(self.phase.poll().map_err(Error::from))? {
            match phase {
                Phase::A(connection) | Phase::B(connection) => Ok(Async::Ready(connection)),
                _ => unreachable!(),
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[derive(Debug)]
pub struct AcquireConnection {
    phase: Phase<Failed<(), Error>, oneshot::Receiver<FutureConnection>, FutureConnection>,
}
impl Future for AcquireConnection {
    type Item = TcpConnection;
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Async::Ready(phase) = track!(self.phase.poll().map_err(Error::from))? {
            let next = match phase {
                Phase::B(future) => Phase::C(future),
                Phase::C(connection) => return Ok(Async::Ready(connection)),
                _ => unreachable!(),
            };
            self.phase = next;
        }
        Ok(Async::NotReady)
    }
}

#[derive(Debug)]
pub struct ConnectionPool {
    //    max_connections: usize,
    // TODO: add timeout
    connections: HashMap<SocketAddr, Vec<TcpConnection>>,
    command_tx: mpsc::Sender<Command>,
    command_rx: mpsc::Receiver<Command>,
}
impl ConnectionPool {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        ConnectionPool {
            //max_connections: 512,
            connections: HashMap::new(),
            command_tx,
            command_rx,
        }
    }
    pub fn handle(&self) -> ConnectionPoolHandle {
        ConnectionPoolHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::AcquireConnection { addr, reply } => {
                if let Some(availables) = self.connections.get_mut(&addr) {
                    let connection = availables.pop().expect("Never fails");
                    let _ = reply.send(FutureConnection::from_connection(connection));
                } else {
                    let _ = reply.send(FutureConnection::connect(addr));
                }
                if self.connections.get(&addr).map_or(false, |x| x.is_empty()) {
                    self.connections.remove(&addr);
                }
            }
            Command::ReleaseConnection { addr, connection } => {
                self.connections
                    .entry(addr)
                    .or_insert_with(Vec::new)
                    .push(connection);
            }
        }
    }
}
impl Future for ConnectionPool {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(command) = self.command_rx.poll().expect("Never fails") {
            let command = command.expect("Never fails");
            self.handle_command(command);
        }
        Ok(Async::NotReady)
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionPoolHandle {
    command_tx: mpsc::Sender<Command>,
}
impl ConnectionPoolHandle {
    pub fn acquire_connection(&self, addr: SocketAddr) -> AcquireConnection {
        let (reply, reply_rx) = oneshot::channel();
        let command = Command::AcquireConnection { addr, reply };

        let phase = if let Err(e) = track!(self.command_tx.send(command).map_err(Error::from)) {
            Phase::A(futures::failed(e))
        } else {
            Phase::B(reply_rx)
        };
        AcquireConnection { phase }
    }
    pub fn release_connection(&self, addr: SocketAddr, connection: TcpConnection) {
        let command = Command::ReleaseConnection { addr, connection };
        let _ = self.command_tx.send(command);
    }
}
