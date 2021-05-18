use std::io;

pub enum State {
	Closed,
	Listen,
	SynRcvd,
	// Estab,
}

pub struct Connection {
	state: State,
	send: SendSequenceSpace,
	recv: RecvSequenceSpace,
}

/// State of the Send Sequence Space (RFC 793 S3.2 F4)
///
/// ```
///            1         2          3          4
///       ----------|----------|----------|----------
///              SND.UNA    SND.NXT    SND.UNA
///                                   +SND.WND
///
/// 1 - old sequence numbers which have been acknowledged
/// 2 - sequence numbers of unacknowledged data
/// 3 - sequence numbers allowed for new data transmission
/// 4 - future sequence numbers which are not yet allowed
/// ```
struct SendSequenceSpace {
	 /// send unacknowledged
	 una: u32,
	 /// send next
	 nxt: u32,
	 /// send window
	 wnd: u16,
	 /// send urgent pointer
	 up: bool,
	 /// segment sequence number used for last window update
	 wl1: usize,
	 /// segment acknowledgment number used for last window update
	 wl2: usize,
	 /// initial send sequence number
	 iss: u32,
} 

/// State of the Receive Sequence Space (RFC 793 S3.2 F5)
///
/// ```
///                1          2          3
///            ----------|----------|----------
///                   RCV.NXT    RCV.NXT
///                             +RCV.WND
///
/// 1 - old sequence numbers which have been acknowledged
/// 2 - sequence numbers allowed for new reception
/// 3 - future sequence numbers which are not yet allowed
/// ```
struct RecvSequenceSpace {
	/// recieve next
	nxt: u32,
	/// recieve window
	wnd: u16,
	/// recieve 
	up: bool,
	/// initial sequence number
	irs: u32,
}

impl Connection {
	pub fn accept<'a>(
		nic: &mut tun_tap::Iface,
		iph: etherparse::Ipv4HeaderSlice<'a>, 
		tcph: etherparse::TcpHeaderSlice<'a>, 
		_data: &'a [u8],
	) -> io::Result<Option<Self>> {
		let mut buff = [0u8; 1500];
		if !tcph.syn() {
			return Ok(None);
		}

		let iss = 0;
		let c = Connection {
			state: State::SynRcvd,
			send: SendSequenceSpace {
				iss,
				una: iss + 1,
				nxt: iss + 1,
				wnd: 10,
				up: false,
				wl1: 0,
				wl2: 0,
			},
			recv: RecvSequenceSpace {
				irs: tcph.sequence_number(),
				nxt: tcph.sequence_number() + 1,
				wnd: tcph.window_size(),
				up: false
			}
		};

		// need to start establishing a connection
		let mut syn_ack = etherparse::TcpHeader::new(
			tcph.destination_port(), 
			tcph.source_port(), 
			c.send.iss, 
			c.send.wnd
		);

		syn_ack.acknowledgment_number = c.recv.nxt;
		syn_ack.syn = true;
		syn_ack.ack = true;

		let ip = etherparse::Ipv4Header::new(
			syn_ack.header_len(), 
			64, 
			etherparse::IpTrafficClass::Tcp, 
			[
				iph.destination()[0],
				iph.destination()[1],
				iph.destination()[2],
				iph.destination()[3],
			],
			[
				iph.source()[0],
				iph.source()[1],
				iph.source()[2],
				iph.source()[3],
			],
		);
		
		syn_ack.checksum = syn_ack
			.calc_checksum_ipv4(&ip, &[])
			.expect("fail to compute checksum");

		// write out the headers
		let unwritten = {
			let mut unwritten = &mut buff[..];
			ip.write(&mut unwritten);
			syn_ack.write(&mut unwritten);
			unwritten.len()
		};

		nic.send(&buff[..unwritten])?;
		Ok(Some(c))
	}

	pub fn on_packet<'a>(
		&mut self,
		nic: &mut tun_tap::Iface,
		iph: etherparse::Ipv4HeaderSlice<'a>, 
		tcph: etherparse::TcpHeaderSlice<'a>, 
		_data: &'a [u8],
	) -> io::Result<()> {
		unimplemented!();
	}
}
