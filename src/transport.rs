use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use h3_webtransport::session::SendStream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed, LengthDelimitedCodec};

use crate::error::Error;

/// Message-oriented stream for bidirectional communication
pub struct MessageStream {
    /// Framed stream for reading and writing length-delimited messages
    framed: Framed<h3_webtransport::session::BidiStream, LengthDelimitedCodec>,
}

impl MessageStream {
    /// Creates a new MessageStream from a WebTransport bidirectional stream
    pub fn new(stream: h3_webtransport::session::BidiStream) -> Self {
        // Configure length-delimited codec for message framing
        let codec = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(4)
            .length_adjustment(0)
            .max_frame_length(16 * 1024 * 1024) // 16MB max message size
            .new_codec();
        
        Self {
            framed: Framed::new(stream, codec),
        }
    }
    
    /// Sends a message over the stream
    pub async fn send(&mut self, bytes: Bytes) -> Result<(), Error> {
        self.framed.send(bytes).await
            .map_err(|e| Error::WebTransport(format!("Failed to send message: {}", e)))
    }
    
    /// Receives a message from the stream
    pub async fn receive(&mut self) -> Result<Option<Bytes>, Error> {
        match self.framed.next().await {
            Some(Ok(bytes)) => Ok(Some(bytes)),
            Some(Err(e)) => Err(Error::WebTransport(format!("Failed to receive message: {}", e))),
            None => Ok(None),
        }
    }
}

/// Codec for Protocol Buffers messages
pub struct ProtobufCodec<T> {
    /// Phantom data to use the type parameter
    _marker: std::marker::PhantomData<T>,
}

impl<T> ProtobufCodec<T> {
    /// Creates a new ProtobufCodec
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Default for ProtobufCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Decoder for ProtobufCodec<T>
where
    T: prost::Message + Default,
{
    type Item = T;
    type Error = Error;
    
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        
        match T::decode(src.as_ref()) {
            Ok(item) => {
                // Consume the entire buffer since we decoded successfully
                src.clear();
                Ok(Some(item))
            }
            Err(e) => Err(Error::Decoding(e)),
        }
    }
}

impl<T> Encoder<T> for ProtobufCodec<T>
where
    T: prost::Message,
{
    type Error = Error;
    
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Reserve space for the message
        let len = item.encoded_len();
        dst.reserve(len);
        
        // Encode the message
        item.encode(dst)
            .map_err(Error::Encoding)
    }
}

/// Codec for JSON messages
pub struct JsonCodec<T> {
    /// Phantom data to use the type parameter
    _marker: std::marker::PhantomData<T>,
}

impl<T> JsonCodec<T> {
    /// Creates a new JsonCodec
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Default for JsonCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Decoder for JsonCodec<T>
where
    T: serde::de::DeserializeOwned,
{
    type Item = T;
    type Error = Error;
    
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        
        match serde_json::from_slice(src.as_ref()) {
            Ok(item) => {
                // Consume the entire buffer since we decoded successfully
                src.clear();
                Ok(Some(item))
            }
            Err(e) => Err(Error::Deserialization(e)),
        }
    }
}

impl<T> Encoder<T> for JsonCodec<T>
where
    T: serde::Serialize,
{
    type Error = Error;
    
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Serialize the item to JSON
        let json = serde_json::to_vec(&item)
            .map_err(Error::Serialization)?;
        
        // Write the JSON to the buffer
        dst.reserve(json.len());
        dst.put_slice(&json);
        
        Ok(())
    }
}