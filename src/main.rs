use std::io;

fn main() -> io::Result<()> {
	let nic = tun_tap::Iface::new("tun0", tun_tap::Mode::Tun)?;
	let mut buff = [0u8; 1504];

	loop {
		let nbytes = nic.recv(&mut buff[..])?;
		let eth_flags = u16::from_be_bytes([buff[0], buff[1]]);
		let eth_proto = u16::from_be_bytes([buff[2], buff[3]]);

		if eth_proto != 0x0800 {
			// not ipv4
			continue;
		}

		match etherparse::Ipv4HeaderSlice::from_slice(&buff[4..nbytes]) {
			Ok(p) => {
				let src = p.source_addr();
				let dst = p.destination_addr();
				let proto = p.protocol();
				eprintln!("{} => {} {}b of protocol {} ", src, dst, proto, p.payload_len())
			},
			
			Err(e) => {
				eprintln!("ignoring weird package {:?}", e);
			}
		};
	};

	Ok(())
}
