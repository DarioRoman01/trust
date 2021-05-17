use std::io;
use std::collections::HashMap;
use std::net::Ipv4Addr;
mod tcp;


#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct Quad {
	src: (Ipv4Addr, u16),
	dst: (Ipv4Addr, u16),

}

fn main() -> io::Result<()> {
	let mut connections: HashMap<Quad, tcp::State> = Default::default();
	let nic = tun_tap::Iface::new("tun0", tun_tap::Mode::Tun)?;
	let mut buff = [0u8; 1504];

	loop {
		let nbytes = nic.recv(&mut buff[..])?;
		let _eth_flags = u16::from_be_bytes([buff[0], buff[1]]);
		let eth_proto = u16::from_be_bytes([buff[2], buff[3]]);

		if eth_proto != 0x0800 {
			// not ipv4
			continue;
		}

		match etherparse::Ipv4HeaderSlice::from_slice(&buff[4..nbytes]) {
			Ok(iph) => { // ip header
				let src = iph.source_addr();
				let dst = iph.destination_addr();
				if iph.protocol() != 0x06 {
					// not tcp  
					continue;
				}
				
				match etherparse::TcpHeaderSlice::from_slice(&buff[4+iph.slice().len()..]) {
					Ok(tcph) => { // tcp header
						let datai = 4 + iph.slice().len() + tcph.slice().len();
						connections.entry(Quad {
							src: (src, tcph.source_port()),
        			dst: (dst, tcph.destination_port()),
						}).or_default().on_packet(iph, tcph, &buff[datai..nbytes]);
					},
					Err(e) => {
						eprintln!("ignoring weird package {:?}", e);
					}
				}
			},
			Err(e) => {
				eprintln!("ignoring weird package {:?}", e);
			}
		};
	};
}
