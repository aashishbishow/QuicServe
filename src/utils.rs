use std::path::Path;
use std::fs;
use std::io::Read;
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::future::Future;
use log::{debug, error, info, warn};
use rustls::Certificate;
use tokio::time;

use crate::error::Error;
use crate::SerializationFormat;

/// Reads a PEM certificate file and returns the certificate data
pub fn read_certificate_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path)
        .map_err(|e| Error::CertificateError(format!("Failed to open certificate file: {}", e)))?;
    
    let mut cert_data = Vec::new();
    file.read_to_end(&mut cert_data)
        .map_err(|e| Error::CertificateError(format!("Failed to read certificate file: {}", e)))?;
    
    Ok(cert_data)
}

/// Parses a PEM certificate into rustls certificates
pub fn parse_certificates(cert_data: &[u8]) -> Result<Vec<Certificate>, Error> {
    rustls_pemfile::certs(&mut cert_data.as_ref())
        .map_err(|e| Error::CertificateError(format!("Failed to parse certificate: {}", e)))
        .map(|certs| certs.into_iter().map(Certificate).collect())
}

/// Parses a serialization format from a string
pub fn parse_format(format_str: &str) -> Result<SerializationFormat, Error> {
    match format_str.to_lowercase().as_str() {
        "protobuf" | "proto" => Ok(SerializationFormat::Protobuf),
        "json" => Ok(SerializationFormat::Json),
        _ => Err(Error::InvalidConfig(format!("Unknown serialization format: {}", format_str))),
    }
}

/// Executes a future with retry logic
pub async fn retry_with_backoff<F, Fut, T>(
    f: F,
    initial_delay: Duration,
    max_delay: Duration,
    max_retries: usize,
) -> Result<T, Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut delay = initial_delay;
    let mut retry_count = 0;
    
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                retry_count += 1;
                
                if retry_count >= max_retries {
                    return Err(err);
                }
                
                debug!("Retry attempt {} after error: {}", retry_count, err);
                
                // Exponential backoff with jitter
                time::sleep(delay).await;
                
                // Calculate next delay with jitter (80-120% of doubled delay)
                let jitter_factor = 0.8 + (rand::random::<f64>() * 0.4);
                delay = std::cmp::min(
                    max_delay,
                    Duration::from_millis((delay.as_millis() as f64 * 2.0 * jitter_factor) as u64)
                );
            }
        }
    }
}

/// Timing wrapper for measuring function execution time
pub async fn timed<F, Fut, T>(name: &str, f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let start = Instant::now();
    let result = f().await;
    let elapsed = start.elapsed();
    debug!("{} completed in {:.2}ms", name, elapsed.as_secs_f64() * 1000.0);
    result
}

/// Creates a standardized service method name
pub fn format_method_name(service: &str, method: &str) -> String {
    format!("{}.{}", service, method)
}

/// Checks if a bidirectional QUIC stream is still alive
pub async fn check_stream_alive(stream: &mut h3_webtransport::session::BidiStream) -> bool {
    // This is a minimal ping implementation - just write and read a small message
    let ping_data = &[0u8; 1];
    
    // Try to write to the stream
    if let Err(_) = stream.get_mut().0.write_all(ping_data).await {
        return false;
    }
    
    // Try to read from the stream
    let mut buf = [0u8; 1];
    match stream.get_mut().1.read_exact(&mut buf).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Configures logging based on environment variables
pub fn configure_logging() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,quicserve=debug");
    }
    env_logger::init();
}

/// Validates connection parameters to ensure they're reasonable
pub fn validate_connection_params(
    keep_alive_ms: Option<u64>,
    idle_timeout_ms: Option<u64>,
    max_concurrent_streams: u64,
) -> Result<(), Error> {
    // Check keep-alive interval
    if let Some(keep_alive) = keep_alive_ms {
        if keep_alive < 100 {
            return Err(Error::InvalidConfig(
                "Keep-alive interval must be at least 100ms".into(),
            ));
        }
    }
    
    // Check idle timeout
    if let Some(idle_timeout) = idle_timeout_ms {
        if idle_timeout < 1000 {
            return Err(Error::InvalidConfig(
                "Idle timeout must be at least 1000ms".into(),
            ));
        }
    }
    
    // Check max concurrent streams
    if max_concurrent_streams == 0 || max_concurrent_streams > 1000 {
        return Err(Error::InvalidConfig(
            "Max concurrent streams must be between 1 and 1000".into(),
        ));
    }
    
    Ok(())
}

/// Returns the local IP addresses of the machine
pub fn get_local_addresses() -> Vec<std::net::IpAddr> {
    match local_ip_address::local_ip() {
        Ok(ip) => vec![ip],
        Err(_) => vec![],
    }
}

/// Simple random request ID generator
pub fn generate_request_id() -> u64 {
    use rand::Rng;
    rand::thread_rng().gen()
}

/// Parses a socket address from a string with default port handling
pub fn parse_socket_addr(addr: &str, default_port: u16) -> Result<std::net::SocketAddr, Error> {
    // Check if the address already has a port
    if addr.contains(':') {
        addr.parse()
            .map_err(|e| Error::InvalidConfig(format!("Invalid socket address: {}", e)))
    } else {
        // Add the default port
        format!("{}:{}", addr, default_port)
            .parse()
            .map_err(|e| Error::InvalidConfig(format!("Invalid socket address: {}", e)))
    }
}