#![allow(non_snake_case)]
//! QuicServe: A high-performance RPC system using WebTransport over HTTP/3
//! 
//! This library provides a framework for building RPC services using
//! WebTransport over HTTP/3, leveraging the performance benefits of QUIC.

#![warn(missing_docs)]
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use h3::quic::Connection;
use h3_webtransport::{server, Session};
use log::{debug, error, info, warn};
use prost::Message;
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time;

// Public modules
pub mod client;
pub mod config;
pub mod error;
pub mod server;
pub mod transport;
pub mod utils;

// Re-exports
pub use client::Client;
pub use error::Error;
pub use server::Server;
pub use transport::Transport;


/// Protocol ID for WebTransport over HTTP/3
pub const WEBTRANSPORT_PROTOCOL: &[u8] = b"webtransport";

/// Default timeout for RPC calls
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Service trait that represents a collection of procedures that can be called remotely
#[async_trait]
pub trait Service: Send + Sync + 'static {
    /// Executes a method on the service
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error>;
    
    /// Returns a list of available methods
    fn methods(&self) -> Vec<String>;
}

/// RPC request type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Unique request ID
    pub id: u64,
    /// Method name to call
    pub method: String,
    /// Serialized payload
    pub payload: Bytes,
}

/// RPC response type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Request ID this response corresponds to
    pub id: u64,
    /// Response payload (if successful)
    pub payload: Option<Bytes>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Serialization format for RPC messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    /// Protocol Buffers
    Protobuf,
    /// JSON
    Json,
}

impl Default for SerializationFormat {
    fn default() -> Self {
        SerializationFormat::Protobuf
    }
}

impl fmt::Display for SerializationFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializationFormat::Protobuf => write!(f, "protobuf"),
            SerializationFormat::Json => write!(f, "json"),
        }
    }
}

/// Serializes data based on the specified format
pub fn serialize<T: Serialize + Message>(
    value: &T,
    format: SerializationFormat,
) -> Result<Bytes, Error> {
    match format {
        SerializationFormat::Json => {
            let json = serde_json::to_vec(value).map_err(Error::Serialization)?;
            Ok(Bytes::from(json))
        }
        SerializationFormat::Protobuf => {
            let mut buf = BytesMut::with_capacity(value.encoded_len());
            value.encode(&mut buf).map_err(Error::Encoding)?;
            Ok(buf.freeze())
        }
    }
}

/// Deserializes data based on the specified format
pub fn deserialize<T: DeserializeOwned + Message + Default>(
    data: &[u8],
    format: SerializationFormat,
) -> Result<T, Error> {
    match format {
        SerializationFormat::Json => {
            serde_json::from_slice(data).map_err(Error::Deserialization)
        }
        SerializationFormat::Protobuf => {
            T::decode(data).map_err(Error::Decoding)
        }
    }
}