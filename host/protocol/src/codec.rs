use std::io;

use futures_util::SinkExt;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::{
    bytes::{Buf, BufMut, Bytes, BytesMut},
    codec::{Decoder, Encoder, FramedRead, FramedWrite},
};

// === Errors === //

#[derive(Debug, Error)]
pub enum CodecError {
    #[error(transparent)]
    Io(io::Error),
    #[error(transparent)]
    Serde(SerdeError),
}

#[derive(Debug, Error)]
pub enum SerdeError {
    #[error("packet length too large ({recv} > {max})")]
    FrameTooBig { recv: u32, max: u32 },
    #[error("malformed message")]
    BadMsg(postcard::Error),
    #[error("message too large")]
    MsgTooLarge,
}

impl From<io::Error> for CodecError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<SerdeError> for CodecError {
    fn from(err: SerdeError) -> Self {
        Self::Serde(err)
    }
}

// === Wrappers === //

pub type FrameDecoder<T> = FramedRead<T, DecodeCodec>;
pub type FrameEncoder<T> = FramedWrite<T, EncodeCodec>;

pub fn wrap_stream_tx<T: AsyncWrite>(tx: T) -> FrameEncoder<T> {
    FramedWrite::new(tx, EncodeCodec)
}

pub fn wrap_stream_rx<T: AsyncRead>(rx: T, max_packet_size: u32) -> FrameDecoder<T> {
    FramedRead::new(rx, DecodeCodec { max_packet_size })
}

pub async fn recv_packet<P: DeserializeOwned>(
    decoder: &mut FrameDecoder<impl AsyncRead + Unpin>,
) -> Result<Option<P>, CodecError> {
    let Some(packet) = tokio_stream::StreamExt::next(decoder).await else {
        return Ok(None);
    };

    let packet = packet?;
    let packet = match postcard::from_bytes(&packet) {
        Ok(v) => v,
        Err(err) => return Err(SerdeError::BadMsg(err).into()),
    };

    Ok(Some(packet))
}

pub async fn feed_packet<P: serde::Serialize>(
    encoder: &mut FrameEncoder<impl AsyncWrite + Unpin>,
    packet: P,
) -> Result<(), CodecError> {
    SinkExt::feed(encoder, packet).await
}

pub async fn send_packet<P: serde::Serialize>(
    encoder: &mut FrameEncoder<impl AsyncWrite + Unpin>,
    packet: P,
) -> Result<(), CodecError> {
    SinkExt::send(encoder, packet).await
}

pub async fn flush_packets(
    encoder: &mut FrameEncoder<impl AsyncWrite + Unpin>,
) -> Result<(), CodecError> {
    SinkExt::<()>::flush(encoder).await
}

// === Codecs === //

#[derive(Debug, Copy, Clone)]
pub struct DecodeCodec {
    pub max_packet_size: u32,
}

impl Decoder for DecodeCodec {
    type Item = Bytes;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, CodecError> {
        if src.len() < 4 {
            return Ok(None);
        }

        let len = {
            let mut tmp = [0u8; 4];
            tmp.copy_from_slice(&src[..4]);
            u32::from_be_bytes(tmp)
        };

        tracing::trace!("decoding packet with size {len}");

        if len > self.max_packet_size {
            return Err(SerdeError::FrameTooBig {
                recv: len,
                max: self.max_packet_size,
            }
            .into());
        }

        let len = len as usize;

        if src.len() < 4 + len {
            src.reserve(4 + len - src.len());
            return Ok(None);
        }

        let mut packet = src.split_to(4 + len);
        packet.advance(4);

        Ok(Some(packet.freeze()))
    }
}

pub struct EncodeCodec;

impl<I: serde::Serialize> Encoder<I> for EncodeCodec {
    type Error = CodecError;

    fn encode(&mut self, item: I, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u32(0xABADF00D);

        struct ExtendAdapter<'a, E>(&'a mut E);

        impl<T, E: Extend<T>> Extend<T> for ExtendAdapter<'_, E> {
            fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
                self.0.extend(iter);
            }
        }

        if let Err(err) = postcard::to_extend(&item, ExtendAdapter(dst)) {
            return Err(SerdeError::BadMsg(err).into());
        };

        let Ok(len) = u32::try_from(dst.len() - 4) else {
            return Err(SerdeError::MsgTooLarge.into());
        };

        dst[0..4].copy_from_slice(&len.to_be_bytes());

        tracing::trace!("encoded packet with size {len:?}");

        Ok(())
    }
}
