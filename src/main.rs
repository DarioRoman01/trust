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
	let mut connections: HashMap<Quad, tcp::Connection> = Default::default();
	let mut nic = tun_tap::Iface::without_packet_info("tun0", tun_tap::Mode::Tun)?;
	let mut buff = [0u8; 1504];

	loop {
		let nbytes = nic.recv(&mut buff[..])?;
		// let _eth_flags = u16::from_be_bytes([buff[0], buff[1]]);
		// let eth_proto = u16::from_be_bytes([buff[2], buff[3]]);

		// if eth_proto != 0x0800 {
		// 	// not ipv4
		// 	continue;
		// }

		match etherparse::Ipv4HeaderSlice::from_slice(&buff[..nbytes]) {
			Ok(iph) => { // ip header
				let src = iph.source_addr();
				let dst = iph.destination_addr();
				if iph.protocol() != 0x06 {
					// not tcp
					continue;
				}

				match etherparse::TcpHeaderSlice::from_slice(&buff[iph.slice().len()..nbytes]) {
					Ok(tcph) => { // tcp header
						use std::collections::hash_map::Entry;
						let datai = iph.slice().len() + tcph.slice().len();
						
						match connections.entry(Quad {
							src: (src, tcph.source_port()),
							dst: (dst, tcph.destination_port()),
						}) {
							Entry::Occupied(mut c) => {
								c.get_mut().on_packet(&mut nic, iph, tcph, &buff[datai..nbytes])?;
							},
							Entry::Vacant(e) => {
								if let Some(c) = tcp::Connection::accept(&mut nic, iph, tcph, &buff[datai..nbytes])? {
									e.insert(c);
								}
							}
						}
						
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
