use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use quinn::{ClientConfig, ServerConfig, TransportConfig};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::SerializationFormat;

/// Configuration for QuicServe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Address to bind to for server or connect to for client
    pub addr: SocketAddr,
    
    /// TLS certificate file path (PEM format)
    pub cert_path: Option<PathBuf>,
    
    /// TLS private key file path (PEM format)
    pub key_path: Option<PathBuf>,
    
    /// Root CA certificate file path for verification (PEM format)
    pub ca_path: Option<PathBuf>,
    
    /// Whether to verify peer certificates
    pub verify_peer: bool,
    
    /// Serialization format
    pub format: SerializationFormat,
    
    /// Timeout for RPC calls in milliseconds
    pub timeout_ms: u64,
    
    /// Maximum concurrent streams per connection
    pub max_concurrent_streams: u64,
    
    /// Keep-alive interval in milliseconds
    pub keep_alive_ms: Option<u64>,
    
    /// Maximum idle timeout in milliseconds
    pub idle_timeout_ms: Option<u64>,
    
    /// Server name for TLS verification
    pub server_name: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "[::1]:4433".parse().unwrap(),
            cert_path: None,
            key_path: None,
            ca_path: None,
            verify_peer: true,
            format: SerializationFormat::Protobuf,
            timeout_ms: crate::DEFAULT_TIMEOUT_MS,
            max_concurrent_streams: 100,
            keep_alive_ms: Some(5000),
            idle_timeout_ms: Some(30000),
            server_name: None,
        }
    }
}

impl Config {
    /// Creates a new configuration with default values
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            ..Default::default()
        }
    }
    
    /// Builds QUIC client configuration
    pub fn build_client_config(&self) -> Result<ClientConfig, Error> {
        let mut client_config = if let Some(ca_path) = &self.ca_path {
            // Load CA certificates
            let ca_cert = std::fs::read(ca_path)
                .map_err(|e| Error::CertificateError(format!("Failed to read CA cert: {}", e)))?;
            
            // Parse CA certificate
            let cert = rustls::Certificate(ca_cert);
            let mut cert_store = rustls::RootCertStore::empty();
            cert_store.add(&cert)
                .map_err(|e| Error::CertificateError(format!("Failed to add CA cert: {}", e)))?;
            
            // Create client config with custom root store
            let mut crypto = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(cert_store)
                .with_no_client_auth();
            
            // Set server name if provided
            if let Some(server_name) = &self.server_name {
                crypto.enable_sni = true;
            }
            
            quinn::ClientConfig::new(Arc::new(crypto))
        } else {
            // Use native root certificates
            quinn::ClientConfig::with_native_roots()
        };
        
        // Configure transport parameters
        let mut transport_config = TransportConfig::default();
        
        // Set keep-alive interval if specified
        if let Some(keep_alive_ms) = self.keep_alive_ms {
            transport_config.keep_alive_interval(Some(Duration::from_millis(keep_alive_ms)));
        }
        
        // Set idle timeout if specified
        if let Some(idle_timeout_ms) = self.idle_timeout_ms {
            transport_config.max_idle_timeout(Some(Duration::from_millis(idle_timeout_ms).try_into().unwrap()));
        }
        
        // Apply transport configuration
        client_config.transport_config(Arc::new(transport_config));
        
        Ok(client_config)
    }
    
    /// Builds QUIC server configuration
    pub fn build_server_config(&self) -> Result<ServerConfig, Error> {
        // Check if certificate and key paths are provided
        let cert_path = self.cert_path.as_ref()
            .ok_or_else(|| Error::InvalidConfig("Certificate file path is required for server".into()))?;
        let key_path = self.key_path.as_ref()
            .ok_or_else(|| Error::InvalidConfig("Private key file path is required for server".into()))?;
        
        // Load certificate chain
        let cert_chain = std::fs::read(cert_path)
            .map_err(|e| Error::CertificateError(format!("Failed to read certificate file: {}", e)))?;
        
        // Load private key
        let key_bytes = std::fs::read(key_path)
            .map_err(|e| Error::CertificateError(format!("Failed to read private key file: {}", e)))?;
        
        // Create server config
        let server_config = quinn::ServerConfig::with_single_cert(
            rustls_pemfile::certs(&mut cert_chain.as_slice())
                .map_err(|e| Error::CertificateError(format!("Failed to parse certificate: {}", e)))?
                .into_iter()
                .map(rustls::Certificate)
                .collect(),
            rustls::PrivateKey(
                rustls_pemfile::pkcs8_private_keys(&mut key_bytes.as_slice())
                    .map_err(|e| Error::CertificateError(format!("Failed to parse private key: {}", e)))?
                    .remove(0),
            ),
        )
        .map_err(|e| Error::CertificateError(format!("Invalid certificate: {}", e)))?;
        
        // Configure transport parameters
        let mut transport_config = TransportConfig::default();
        
        // Set keep-alive interval if specified
        if let Some(keep_alive_ms) = self.keep_alive_ms {
            transport_config.keep_alive_interval(Some(Duration::from_millis(keep_alive_ms)));
        }
        
        // Set idle timeout if specified
        if let Some(idle_timeout_ms) = self.idle_timeout_ms {
            transport_config.max_idle_timeout(Some(Duration::from_millis(idle_timeout_ms).try_into().unwrap()));
        }
        
        // Set max concurrent streams
        transport_config.max_concurrent_uni_streams(self.max_concurrent_streams.try_into().unwrap());
        
        // Apply transport configuration
        let mut server_config = server_config.clone();
        server_config.transport_config(Arc::new(transport_config));
        
        Ok(server_config)
    }
}