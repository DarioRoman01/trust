use std::io;
use std::io::prelude::*;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::thread;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct Quad {
	src: (Ipv4Addr, u16),
	dst: (Ipv4Addr, u16),
}

type InterfaceHandle = mpsc::Sender<InterfaceRequest>;

enum InterfaceRequest {
    Write{
        quad: Quad,
        bytes: Vec<u8>, 
        ack: mpsc::Sender<usize>
    },
    Flush{
        quad: Quad,
        ack: mpsc::Sender<()>
    },
    Bind{
        port: u16, 
        ack: mpsc::Sender<Vec<u8>>
    },
    Unbind,
    Read{
        quad: Quad,
        max_len: usize, 
        read: mpsc::Sender<Vec<u8>>
    },
    Accept{
        port: u16,
        read: mpsc::Sender<Quad>
    },
}

pub struct Interface {
    tx: InterfaceHandle,
    jh: thread::JoinHandle<()>
}

struct ConnectionManager {
    connections: HashMap<Quad, tcp::Connection>,
    nic: tun_tap::Iface,
    buff: [u8; 1504],
}

impl ConnectionManager {
    fn run_on(self, rx: mpsc::Sender<InterfaceRequest>) {
        // main event loop for packet processing
        for req in rx {

        }
    }
}

impl Interface {
    pub fn new() -> io::Result<Self> {
        let cm = ConnectionManager {
            connections: Default::default(), 
            nic: tun_tap::Iface::without_packet_info("tun0", tun_tap::Mode::Tun)?, 
            buff: [0u8; 1504],
        };

        let (tx, rx) = mpsc::channel();
        let jh = thread::spawn(move || cm.run_on(rx));
        Ok(Interface { tx, jh})
    }

    pub fn bind(&mut self, port: u16) -> io::Result<TcpListener> {
        let (ack, rx) = mpsc::channel();
        self.tx.send(InterfaceRequest::Bind {
            port,
            ack
        });
        rx.recv().unwrap();
        Ok(TcpListener(port, self.tx.clone()))
    }
}

pub struct TcpStream(Quad, InterfaceHandle);

impl Read for TcpStream {
    fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> { 
        let (read, rx) = mpsc::channel();
        self.1.send(InterfaceRequest::Read {
            quad: self.0,
            max_len: buff.len(),
            read
        });

        let bytes = rx.recv().unwrap();
        assert!(bytes.len() <= buff.len());
        buff.copy_from_slice(&bytes[..]);
        Ok(bytes.len())
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
        let (ack, rx) = mpsc::channel();
        self.1.send(InterfaceRequest::Accept {
            port: self.0,
            read: ack,
        });

        let quad = rx.recv().unwrap();
        Ok(TcpStream(quad, self.1.clone()))
    }
}