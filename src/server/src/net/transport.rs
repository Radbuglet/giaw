use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::SinkExt;
use giaw_shared::game::services::rpc::{decode_packet, encode_packet, RpcPacket};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed};

pub struct QuadNetStream {
    stream: Framed<TcpStream, QuadNetCodec>,
}

impl QuadNetStream {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream: Framed::new(stream, QuadNetCodec),
        }
    }

    pub async fn read(&mut self) -> Option<anyhow::Result<RpcPacket>> {
        self.stream
            .next()
            .await
            .map(|packet| decode_packet(&packet?))
    }

    pub async fn write(&mut self, packet: &RpcPacket) -> anyhow::Result<()> {
        self.stream.send(&encode_packet(packet)).await
    }
}

struct QuadNetCodec;

impl Decoder for QuadNetCodec {
    type Item = Bytes;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some(packet_len) = src.first().map(|v| *v as usize) else {
            return Ok(None);
        };

        if src.len() <= packet_len {
            return Ok(None);
        }

        let packet = src.clone().freeze().slice(1..).slice(..packet_len);
        src.advance(packet_len + 1);

        Ok(Some(packet))
    }
}

impl<'a> Encoder<&'a Bytes> for QuadNetCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: &'a Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u8(u8::try_from(item.len()).unwrap());
        dst.put(&**item);
        Ok(())
    }
}
