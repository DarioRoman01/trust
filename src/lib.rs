use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::prelude::*;
use std::net::{Ipv4Addr, Shutdown};
use std::sync::{Arc, Mutex};
use std::thread;
mod tcp;

const SEND_QUEUE_SIZE: usize = 1024;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct Quad {
    src: (Ipv4Addr, u16),
    dst: (Ipv4Addr, u16),
}

type InterfaceHandle = Arc<Mutex<ConnectionManager>>;

pub struct Interface {
    ih: InterfaceHandle,
    jh: thread::JoinHandle<()>,
}

#[derive(Default)]
struct ConnectionManager {
    connections: HashMap<Quad, tcp::Connection>,
    pending: HashMap<u16, VecDeque<Quad>>,
}

fn packet_loop(mut nic: tun_tap::Iface, ih: InterfaceHandle) {
    let mut buf = [0u8; 1504];
    loop {
        let nbytes = nic.recv(&mut buf[..])?;

        match etherparse::Ipv4HeaderSlice::from_slice(&buf[..nbytes]) {
            Ok(tcph) => {
                use std::collections::hash_map::Entry;
                let datai = iph
            },
            Err(_) => todo!(),
        }
    }
}

impl Interface {
    pub fn new() -> io::Result<Self> {
        let nic = tun_tap::Iface::without_packet_info("tun0", tun_tap::Mode::Tun)?;
        let ih: InterfaceHandle = Arc::default();

        let jh = {
            let ih = ih.clone();
            thread::spawn(move || packet_loop(nic, ih));
        };

        Ok(Interface { ih, jh })
    }

    pub fn bind(&mut self, port: u16) -> io::Result<TcpListener> {
        use std::collections::hash_map::Entry;
        let mut cm = self.ih.lock().unwrap();

        match cm.pending.entry(port) {
            Entry::Vacant(v) => v.insert(VecDeque::new()),
            Entry::Occupied(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "port already bound",
                ))
            }
        };

        // TODO: something to start accepting SYN packets on `PORT`
        drop(cm);
        Ok(TcpListener{port, h: self.ih.clone()})
    }
}

pub struct TcpStream {
    quad: Quad, 
    ih: InterfaceHandle
}

impl Read for TcpStream {
    fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> {
        let mut cm = self.ih.lock().unwrap();
        let c = cm.connections.get_mut(&self.quad).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "stram was terminated unexpectedly",
            )
        })?;

        if c.incoming.is_empty() {
            // TODO: block
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "there is no bytes to read",
            ));
        }

        // TODO: detect FIN and return nread == 0
        let mut nread = 0;
        let (head, tail) = c.incoming.as_slices();

        let hread = std::cmp::min(buff.len(), head.len());
        buff.copy_from_slice(&head[..hread]);
        nread += hread;

        let tread = std::cmp::min(buff.len() - nread, tail.len());
        buff.copy_from_slice(&tail[..tread]);
        nread += tread;

        drop(c.incoming.drain(..nread));
        Ok(nread)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buff: &[u8]) -> io::Result<usize> {
        let mut cm = self.ih.lock().unwrap();
        let c = cm.connections.get_mut(&self.quad).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "stram was terminated unexpectedly",
            )
        })?;

        if c.unacked.len() >= SEND_QUEUE_SIZE {
            // TODO: block
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "too many bytes buffered",
            ));
        }

        let nwrite = std::cmp::min(buff.len(), SEND_QUEUE_SIZE - c.unacked.len());
        c.unacked.extend(&buff[..nwrite]);
        Ok(nwrite)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut cm = self.ih.lock().unwrap();
        let c = cm.connections.get_mut(&self.quad).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "stram was terminated unexpectedly",
            )
        })?;

        if c.unacked.is_empty() {
            return Ok(());
        } else {
            // TODO: block
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "too many bytes buffered",
            ));
        }
    }
}

impl TcpStream {
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        unimplemented!()
    }
}

pub struct TcpListener {
    port: u16, 
    h: InterfaceHandle
}

impl TcpListener {
    pub fn accept(&mut self) -> io::Result<TcpStream> {
        let mut cm = self.h.lock().unwrap();
        if let Some(quad) = cm
            .pending
            .get_mut(&self.port)
            .expect("port closed while liststener still active")
            .pop_front()
        {
            return Ok(TcpStream{quad, ih: self.h.clone()});
        } else {
            // TODO: block
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "no connection to accpet",
            ));
        }
    }
}
