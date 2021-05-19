use std::{cmp::min, io};

pub enum State {
	// Listen,
	SynRcvd,
	Estab,
}

impl State {
	fn is_synchronized(&self) -> bool {
		match *self {
			State::SynRcvd => false,
			State::Estab => true
		}
	}
}

pub struct Connection {
	state: State,
	send: SendSequenceSpace,
	recv: RecvSequenceSpace,
	ip: etherparse::Ipv4Header,
	tcp: etherparse::TcpHeader,
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
		
		// let buff = [0u8; 1500];
		if !tcph.syn() {
				return Ok(None);
		}

		let iss = 0;
		let wnd = 10;
		let mut c = Connection {
			state: State::SynRcvd,
			send: SendSequenceSpace {
				iss,
				una: iss,
				nxt: iss,
				wnd: 10,
				up: false,
				wl1: 0,
				wl2: 0,
			},
			recv: RecvSequenceSpace {
				irs: tcph.sequence_number(),
				nxt: tcph.sequence_number() + 1,
				wnd: tcph.window_size(),
				up: false,
			},
			ip: etherparse::Ipv4Header::new(
				0,
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
			),
			tcp: etherparse::TcpHeader::new(
				tcph.destination_port(),
				tcph.source_port(),
				iss,
				wnd,
			),
		};

		c.tcp.syn = true;
		c.tcp.ack = true;
		c.write(nic, &[])?;
		Ok(Some(c))
	}

	fn write(&mut self, nic: &mut tun_tap::Iface, payload: &[u8]) -> io::Result<usize> {
		use std::io::Write;

		let mut buff = [0u8; 1500];
		self.tcp.sequence_number = self.send.nxt;
		self.tcp.acknowledgment_number = self.recv.nxt;
		let size = min(buff.len(), self.tcp.header_len() as usize + self.ip.header_len() + payload.len());

		self.ip.set_payload_len(size).expect("errror setting ip payload");

		let mut unwritten = &mut buff[..];
		self.ip.write(&mut unwritten).expect("error writen ip header");
		self.tcp.write(&mut unwritten).expect("error writen syn");
		let payload_bytes = unwritten.write(payload)?;
		let unwritten = unwritten.len();

		self.send.nxt = self.send.nxt.wrapping_add(payload_bytes as u32);
		if self.tcp.syn {
			self.send.nxt = self.send.nxt.wrapping_add(1);
			self.tcp.syn = false;
		}

		if self.tcp.fin {
			self.send.nxt = self.send.nxt.wrapping_add(1);
			self.tcp.fin = false; 
		}

		nic.send(&buff[..buff.len() - unwritten])?;
		Ok(payload_bytes)
	}

	pub fn send_rst(&mut self, nic: &mut tun_tap::Iface) -> io::Result<()> {
		self.tcp.rst = true;
		self.tcp.sequence_number = 0;
		self.tcp.acknowledgment_number = 0;
		self.write(nic, &[])?;
		Ok(())
	}

	pub fn on_packet<'a>(
		&mut self,
		nic: &mut tun_tap::Iface,
		iph: etherparse::Ipv4HeaderSlice<'a>,
		tcph: etherparse::TcpHeaderSlice<'a>,
		data: &'a [u8],
	) -> io::Result<()> {

		let ackn = tcph.acknowledgment_number();
		if !is_between_wrapped(self.send.una, ackn, self.send.nxt.wrapping_add(1)) {
			if !self.state.is_synchronized() {
				self.send_rst(nic)?;
			}
			return Ok(());
		}

		// valid segment check
		
		let seqn = tcph.sequence_number();
		let wend = self.recv.nxt.wrapping_add(self.recv.wnd as u32);
		let mut slen = data.len() as u32;
		if tcph.fin() { slen += 1 }
		if tcph.syn() { slen += 1 }

		if slen == 0 && !tcph.syn() && !tcph.fin() {
			// zero length segment has separate rules for acceptance
			if self.recv.wnd == 0 {
				if seqn != self.recv.nxt {
					return Ok(());
				}
			} else if !is_between_wrapped(self.recv.nxt.wrapping_sub(1), seqn, wend) {
					return Ok(());
			}
		} else {
			if self.recv.wnd == 0 {
				return Ok(());
			} else if !is_between_wrapped(self.recv.nxt.wrapping_sub(1), seqn + slen - 1 , wend) {
					return Ok(());
			}
		}

		match self.state {
			State::SynRcvd => {
				// expect to get an ACK from the SYN
				if !tcph.ack() {
					return Ok(());
				}

				self.state = State::Estab;


			}
			State::Estab => {
				unimplemented!();
			}
		}
		Ok(())
	}
}

/// acceptable ACK check
/// SND.UNA < SEG.ACK =< SND.NEXT
///  start  <    x    =<   end 
fn is_between_wrapped(start: u32, x: u32, end: u32) -> bool {
	use std::cmp::Ordering;

	match start.cmp(&x) {
		Ordering::Equal => return false,

		Ordering::Less => {
			if end >= start && end <= x {
				return false;
			} else {
				return true;
			}
		}

		Ordering::Greater => {
			if end < start && end > x {
				return true;
			} else {
				return false;
			}
		}
	}
}
