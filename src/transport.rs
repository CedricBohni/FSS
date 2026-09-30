//! Message transport between the two parties.

use crate::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub trait Channel {
    /// Send `mine` to the peer and return what the peer sent in the same round.
    fn exchange(&mut self, mine: &[u128]) -> Result<Vec<u128>, Error>;
}

/// TCP connection to the other party. Messages are length-prefixed bincode vectors.
pub struct TcpChannel {
    stream: TcpStream,
    party: u8,
}

impl TcpChannel {
    /// Party 0 waits for party 1 to connect.
    pub fn listen(addr: impl ToSocketAddrs) -> Result<TcpChannel, Error> {
        TcpChannel::accept(&TcpListener::bind(addr)?)
    }

    /// Party 0 on an already bound listener (e.g. port 0, to let the OS pick a free port).
    pub fn accept(listener: &TcpListener) -> Result<TcpChannel, Error> {
        let (stream, _) = listener.accept()?;
        stream.set_nodelay(true)?;
        Ok(TcpChannel { stream, party: 0 })
    }

    /// Party 1 connects to party 0, retrying until `timeout` so start order does not matter.
    pub fn connect(addr: impl ToSocketAddrs + Clone, timeout: Duration) -> Result<TcpChannel, Error> {
        let start = Instant::now();
        loop {
            match TcpStream::connect(addr.clone()) {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    return Ok(TcpChannel { stream, party: 1 });
                }
                Err(e) if start.elapsed() < timeout => {
                    let _ = e;
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn send(&mut self, msg: &[u128]) -> Result<(), Error> {
        let bytes = bincode::serialize(msg)?;
        self.stream.write_all(&(bytes.len() as u64).to_le_bytes())?;
        self.stream.write_all(&bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Receive a vector of at most `max_items` values. The length prefix is checked before
    /// allocating, so a faulty peer cannot make us reserve arbitrary memory.
    fn recv(&mut self, max_items: usize) -> Result<Vec<u128>, Error> {
        let mut len = [0u8; 8];
        self.stream.read_exact(&mut len)?;
        let len = u64::from_le_bytes(len);
        // bincode Vec<u128>: u64 length + 16 bytes per element.
        let max_len = 8 + 16 * max_items as u64;
        if len > max_len {
            return Err(Error::new(format!("peer announced a {len}-byte message, expected at most {max_len}")));
        }
        let mut bytes = vec![0u8; len as usize];
        self.stream.read_exact(&mut bytes)?;
        Ok(bincode::deserialize(&bytes)?)
    }
}

impl Channel for TcpChannel {
    fn exchange(&mut self, mine: &[u128]) -> Result<Vec<u128>, Error> {
        // Fixed order, so large messages cannot deadlock on full socket buffers.
        if self.party == 0 {
            self.send(mine)?;
            self.recv(mine.len())
        } else {
            let theirs = self.recv(mine.len())?;
            self.send(mine)?;
            Ok(theirs)
        }
    }
}

/// In-process channel for running both parties on two threads.
pub struct LocalChannel {
    tx: Sender<Vec<u128>>,
    rx: Receiver<Vec<u128>>,
}

impl LocalChannel {
    pub fn pair() -> (LocalChannel, LocalChannel) {
        let (tx0, rx1) = channel();
        let (tx1, rx0) = channel();
        (LocalChannel { tx: tx0, rx: rx0 }, LocalChannel { tx: tx1, rx: rx1 })
    }
}

impl Channel for LocalChannel {
    fn exchange(&mut self, mine: &[u128]) -> Result<Vec<u128>, Error> {
        self.tx.send(mine.to_vec()).map_err(|_| Error::new("peer hung up"))?;
        self.rx.recv().map_err(|_| Error::new("peer hung up"))
    }
}
