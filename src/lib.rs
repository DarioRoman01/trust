use std::io;
use std::io::prelude::*;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

type InterfaceHandle = mpsc::Sender<InterfaceRequest>;

enum InterfaceRequest {
    Write{
        bytes: Vec<u8>, 
        ack: mpsc::Sender<usize>
    },
    Flush{ack: mpsc::Sender<()>},
    Bind{
        port: u16, 
        ack: mpsc::Sender<Vec<u8>>
    },
    Unbind,
    Read{
        max_len: usize, 
        read: mpsc::Sender<Vec<u8>>
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
    fn run_on(self, rx: InterfaceHandle) {
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
        unimplemented!()
    }
}

pub struct TcpStream(InterfaceHandle);

impl Read for TcpStream {
    fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> { unimplemented!() }
}

impl Write for TcpStream {
    fn write(&mut self, buff: &[u8]) -> io::Result<usize> { unimplemented!() }

    fn flush(&mut self) -> io::Result<()> { unimplemented!() }
}

pub struct TcpListener {}

impl TcpListener {
    pub fn accept(&mut self) -> io::Result<TcpStream> { unimplemented!() }
}