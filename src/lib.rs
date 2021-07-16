use std::io;
use std::io::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
mod tcp;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct Quad {
	src: (Ipv4Addr, u16),
	dst: (Ipv4Addr, u16),
}

type InterfaceHandle = Arc<Mutex<ConnectionManager>>;

pub struct Interface {
    ih: InterfaceHandle,
    jh: thread::JoinHandle<()>
}

#[derive(Default)]
struct ConnectionManager {
    connections: HashMap<Quad, tcp::Connection>,
    pending: HashMap<u16, VecDeque<Quad>>
}

impl Interface {
    pub fn new() -> io::Result<Self> {
        let nic = tun_tap::Iface::without_packet_info("tun0", tun_tap::Mode::Tun)?;
        let cm: InterfaceHandle = Arc::default();

        let jh = {
            let cm = cm.clone();
            thread::spawn(move || {
                let nic = nic;
                let cm = cm;
                let buf = [0u8; 1504]; 
            })
        };
        Ok(Interface { ih: cm, jh})
    }

    pub fn bind(&mut self, port: u16) -> io::Result<TcpListener> {
        use std::collections::hash_map::Entry;
        let cm = self.ih.lock().unwrap();

        match cm.pending.entry(port) {
            Entry::Vacant(v) => v.insert(VecDeque::new()),
            Entry::Occupied(_) => {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "port already bound"))
            },
        };

        // TODO: something to start accepting SYN packets on `PORT`
        drop(cm);
        Ok(TcpListener(port, self.ih.clone()))
    }
}

pub struct TcpStream(Quad, InterfaceHandle);

impl Read for TcpStream {
    fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> { 
        let cm = self.1.lock().unwrap();
        let c = cm.connections.get_mut(&self.0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted, 
                "stram was terminated unexpectedly"
            )
        })?;

        if c.incoming.is_empty() {
            // TODO: block
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted, 
                "there is no bytes to read"
            ));
        }   
    }
}

impl Write for TcpStream {
    fn write(&mut self, buff: &[u8]) -> io::Result<usize> { 
        let (ack, rx) = mpsc::channel();
        self.1.send(InterfaceRequest::Write {
            quad: self.0.clone(),
            bytes: Vec::from(buff),
            ack
        });

        let n = rx.recv().unwrap();
        assert!(n <= buff.len());
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> { 
        let (ack, rx) = mpsc::channel();
        self.1.send(InterfaceRequest::Flush {
            quad: self.0,
            ack
        });
        rx.recv().unwrap();
        Ok(())
    }
}

pub struct TcpListener(u16 ,InterfaceHandle);

impl TcpListener {
    pub fn accept(&mut self) -> io::Result<TcpStream> { 
        let cm = self.1.lock().unwrap();
        if let Some(quad) = cm.pending.get_mut(&self.0).expect("port closed while liststener still active").pop_front() {
            return Ok(TcpStream(quad, self.1.clone()));
        } else {
            // TODO: block
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "no connection to accpet"));
        }
    }
}