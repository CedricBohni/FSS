//! Message transport between the two parties.

use crate::ring::U256;
use crate::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub trait Channel {
    /// Send `mine` to the peer and return what the peer sent in the same round.
    fn exchange(&mut self, mine: &[U256]) -> Result<Vec<U256>, Error>;
}

/// TCP connection to the other party. A message is a u64 element count, a u8 width `w`, and
/// then each element as `w` little-endian bytes, where `w` is the fewest bytes that hold every
/// element (so about `ceil(n/8)` for masked values in Z_{2^n}).
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

    fn send(&mut self, msg: &[U256]) -> Result<(), Error> {
        let width = msg.iter().map(|&x| 32 - x.leading_zeros() as usize / 8).max().unwrap_or(0);
        let mut bytes = Vec::with_capacity(9 + width * msg.len());
        bytes.extend((msg.len() as u64).to_le_bytes());
        bytes.push(width as u8);
        for x in msg {
            bytes.extend(&x.to_le_bytes()[..width]);
        }
        self.stream.write_all(&bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Receive a vector of at most `max_items` values. The header is checked before
    /// allocating, so a faulty peer cannot make us reserve arbitrary memory.
    fn recv(&mut self, max_items: usize) -> Result<Vec<U256>, Error> {
        let mut header = [0u8; 9];
        self.stream.read_exact(&mut header)?;
        let count = u64::from_le_bytes(header[..8].try_into().expect("8 bytes"));
        let width = header[8] as usize;
        if count > max_items as u64 {
            return Err(Error::new(format!("peer announced {count} values, expected at most {max_items}")));
        }
        if width > 32 {
            return Err(Error::new(format!("peer announced {width}-byte values, at most 32 allowed")));
        }
        let mut bytes = vec![0u8; count as usize * width];
        self.stream.read_exact(&mut bytes)?;
        Ok((0..count as usize)
            .map(|i| {
                let mut buf = [0u8; 32];
                buf[..width].copy_from_slice(&bytes[i * width..(i + 1) * width]);
                U256::from_le_bytes(buf)
            })
            .collect())
    }
}

impl Channel for TcpChannel {
    fn exchange(&mut self, mine: &[U256]) -> Result<Vec<U256>, Error> {
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
    tx: Sender<Vec<U256>>,
    rx: Receiver<Vec<U256>>,
}

impl LocalChannel {
    pub fn pair() -> (LocalChannel, LocalChannel) {
        let (tx0, rx1) = channel();
        let (tx1, rx0) = channel();
        (LocalChannel { tx: tx0, rx: rx0 }, LocalChannel { tx: tx1, rx: rx1 })
    }
}

impl Channel for LocalChannel {
    fn exchange(&mut self, mine: &[U256]) -> Result<Vec<U256>, Error> {
        self.tx.send(mine.to_vec()).map_err(|_| Error::new("peer hung up"))?;
        self.rx.recv().map_err(|_| Error::new("peer hung up"))
    }
}
