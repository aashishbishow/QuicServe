use std::io;
use thiserror::Error;

/// Error types for the QuicServe RPC system
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("QUIC connection error: {0}")]
    Quic(String),

    #[error("HTTP/3 error: {0}")]
    Http3(String),

    #[error("WebTransport error: {0}")]
    WebTransport(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] serde_json::Error),

    #[error("Protocol buffer encoding error: {0}")]
    Encoding(#[from] prost::EncodeError),

    #[error("Protocol buffer decoding error: {0}")]
    Decoding(#[from] prost::DecodeError),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Request timeout")]
    Timeout,

    #[error("RPC call failed: {0}")]
    RpcFailed(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Certificate error: {0}")]
    CertificateError(String),

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err.to_string())
    }
}

impl From<quinn::ConnectError> for Error {
    fn from(err: quinn::ConnectError) -> Self {
        Error::Quic(err.to_string())
    }
}

impl From<quinn::ConnectionError> for Error {
    fn from(err: quinn::ConnectionError) -> Self {
        Error::Quic(err.to_string())
    }
}

impl From<h3::Error> for Error {
    fn from(err: h3::Error) -> Self {
        Error::Http3(err.to_string())
    }
}