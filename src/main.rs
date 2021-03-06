const SOFTWARE_NAME: &str = "StunServ";

extern crate bytecodec;
extern crate failure;
extern crate structopt;
extern crate stun_codec;

use bytecodec::{DecodeExt, EncodeExt};
use failure::ensure;
use failure::format_err;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::panic::catch_unwind;
use structopt::StructOpt;
use stun_codec::rfc5389::attributes::{ErrorCode, MappedAddress, Software, XorMappedAddress};
use stun_codec::rfc5389::methods::BINDING;
use stun_codec::rfc5389::Attribute as StunAttribute;
use stun_codec::MessageClass;
use stun_codec::{MessageDecoder, MessageEncoder};

pub type Result<T> = std::result::Result<T, failure::Error>;
pub type StunMessage = stun_codec::Message<StunAttribute>;

#[derive(StructOpt)]
struct Settings {
    /// Return error replies
    #[structopt(long = "fail-replies")]
    fail_replies: bool,
}

#[derive(StructOpt)]
struct Opt {
    /// IP to listen for incoming requests
    #[structopt(short = "-l", default_value = "0.0.0.0")]
    listen_address: IpAddr,

    // UDP port to listen for incoming requests
    #[structopt(short = "-p", default_value = "3479")]
    listen_port: u16,

    #[structopt(flatten)]
    settings: Settings,
}

fn serve(request: StunMessage, mut addr: SocketAddr, settings: &Settings) -> Result<StunMessage> {
    ensure!(
        request.class() == MessageClass::Request,
        "Received a non-request",
    );
    ensure!(
        request.method() == BINDING,
        "Received not a BINDING request",
    );

    if let SocketAddr::V6(a6) = addr {
        if let Some(a4) = a6.ip().to_ipv4() {
            addr = SocketAddr::new(
                std::net::IpAddr::V4(a4),
                addr.port(),
            );
        }
    }

    let mut reply;
    if !settings.fail_replies {
        reply = StunMessage::new(
            MessageClass::SuccessResponse,
            BINDING,
            request.transaction_id(),
        );

        reply.add_attribute(StunAttribute::XorMappedAddress(XorMappedAddress::new(addr)));
        reply.add_attribute(StunAttribute::MappedAddress(MappedAddress::new(addr)));
    } else {
        reply = StunMessage::new(
            MessageClass::ErrorResponse,
            BINDING,
            request.transaction_id(),
        );
        reply.add_attribute(StunAttribute::ErrorCode(ErrorCode::new(
            495,
            "You are not supposed to use your external address now".to_string(),
        )?));
    }

    reply.add_attribute(StunAttribute::Software(Software::new(
        SOFTWARE_NAME.to_string(),
    )?));

    Ok(reply)
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let sa = SocketAddr::from((opt.listen_address, opt.listen_port));
    let udp = UdpSocket::bind(sa)?;

    loop {
        match catch_unwind(|| -> Result<()> {
            let mut buf = [0u8; 1600];
            let (size, addr) = udp.recv_from(&mut buf[..])?;
            let buf = &buf[0..size];

            let mut rq_decoder = MessageDecoder::<StunAttribute>::new();
            let mut rp_encoder = MessageEncoder::<StunAttribute>::new();

            let request = rq_decoder.decode_from_bytes(buf)?;
            let request = request.map_err(|_| format_err!("Broken message"))?;

            let reply = serve(request, addr, &opt.settings)?;

            let reply = rp_encoder.encode_into_bytes(reply)?;
            udp.send_to(&reply[..], addr)?;

            Ok(())
        }) {
            Err(_) => eprintln!("Panic occurred!"),
            Ok(Err(e)) => eprintln!("error: {}", e),
            Ok(Ok(())) => (),
        }
    }
}
