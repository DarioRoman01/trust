use std::{cmp::min, collections::VecDeque, io};

pub enum State {
    // Listen,
    SynRcvd,
    Estab,
    FinW1, // fin wait 1 state
    Finw2, // fin wait 2 state
    TimeWait,
    // CloseW, close wait state
}

impl State {
    fn is_synchronized(&self) -> bool {
        match *self {
            State::SynRcvd => false,
            State::Estab | State::FinW1 | State::Finw2 | State::TimeWait => true,
        }
    }
}

pub struct Connection {
    state: State,
    send: SendSequenceSpace,
    recv: RecvSequenceSpace,
    ip: etherparse::Ipv4Header,
    tcp: etherparse::TcpHeader,
    pub incoming: VecDeque<u8>,
    pub unacked: VecDeque<u8>,
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
            tcp: etherparse::TcpHeader::new(tcph.destination_port(), tcph.source_port(), iss, wnd),
            incoming: Default::default(),
            unacked: Default::default(),
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

        let size = min(
            buff.len(),
            self.tcp.header_len() as usize + self.ip.header_len() + payload.len(),
        );

        self.ip
            .set_payload_len(size - self.ip.header_len() as usize)
            .expect("errror setting ip payload");

        self.tcp.checksum = self
            .tcp
            .calc_checksum_ipv4(&self.ip, &[])
            .expect("Failed to comput checksum");

        let mut unwritten = &mut buff[..];
        self.ip
            .write(&mut unwritten)
            .expect("error writen ip header");

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
        _iph: etherparse::Ipv4HeaderSlice<'a>,
        tcph: etherparse::TcpHeaderSlice<'a>,
        data: &'a [u8],
    ) -> io::Result<()> {
        // valid segment check
        let seqn = tcph.sequence_number();
        let wend = self.recv.nxt.wrapping_add(self.recv.wnd as u32);
        let mut slen = data.len() as u32;
        if tcph.fin() {
            slen += 1
        }
        if tcph.syn() {
            slen += 1
        }

        let ok = if slen == 0 {
            // zero-length segment has separate rules for acceptance
            if self.recv.wnd == 0 {
                if seqn != self.recv.nxt {
                    false
                } else {
                    true
                }
            } else if !is_between_wrapped(self.recv.nxt.wrapping_sub(1), seqn, wend) {
                false
            } else {
                true
            }
        } else {
            if self.recv.wnd == 0 {
                false
            } else if !is_between_wrapped(self.recv.nxt.wrapping_sub(1), seqn, wend)
                && !is_between_wrapped(
                    self.recv.nxt.wrapping_sub(1),
                    seqn.wrapping_add(slen - 1),
                    wend,
                )
            {
                false
            } else {
                true
            }
        };

        if !ok {
            self.write(nic, &[])?;
            return Ok(());
        }

        if !tcph.ack() {
            return Ok(());
        }

        self.recv.nxt = seqn.wrapping_add(slen - 1);
        let ackn = tcph.acknowledgment_number();

        if let State::SynRcvd = self.state {
            if !is_between_wrapped(
                self.send.una.wrapping_sub(1),
                ackn,
                self.send.nxt.wrapping_add(1),
            ) {
                self.state = State::Estab;
            } else {
                // TODO: RST: <SEQ=SEG.ACK><CTL=RST
            }
        }

        if let State::Estab | State::FinW1 | State::Finw2 = self.state {
            if !is_between_wrapped(
                self.send.una.wrapping_sub(1),
                ackn,
                self.send.nxt.wrapping_add(1),
            ) {
                return Ok(());
            }

            self.send.una = ackn;
            assert!(data.is_empty());

            
            if let State::Estab = self.state {
                // finish connection
                self.tcp.fin = true;
                self.write(nic, &[])?;
                self.state = State::FinW1;
            }
        }

        if let State::FinW1 = self.state {
            if self.send.una == self.send.iss + 2 {
                // the FIN has ben ACKed
                self.state = State::Finw2
            }
        }

        if tcph.fin() {
            match self.state {
                State::Finw2 => {
                    // done with the connection
                    self.tcp.fin = true;
                    self.write(nic, &[])?;
                    self.state = State::TimeWait;
                }
                _ => unimplemented!(),
            }
        }

        Ok(())
    }
}

fn wrapping_lt(lhs: u32, rhs: u32) -> bool {
    // From RFC1323:
    //    TCP determines if a data segment is "old" or "new" by testing
    //    whether its sequence number is within 2 ** 31 bytes of the left edge
    //    if the window, and if it is not, discarding the data as "old". To
    //    insure that new data is never mistakenly considered old and viceversa
    //    the left edge of the sender's widnow has to be the most 2 ** 31 away
    //    from the rigth edge reciviers window.
    lhs.wrapping_sub(rhs) > 2 ^ 31
}

/// acceptable ACK check
/// SND.UNA < SEG.ACK =< SND.NEXT
///  start  <    x    =<   end
fn is_between_wrapped(start: u32, x: u32, end: u32) -> bool {
    wrapping_lt(start, x) && wrapping_lt(x, end)
}
