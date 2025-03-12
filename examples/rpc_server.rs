// examples/server.rs
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::{debug, info, warn};
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio::time;

use quicserve::{Config, Error, Server, Service};

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/quicserve.rs"));

// Example service implementation
struct EchoService;

#[async_trait]
impl Service for EchoService {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error> {
        match method {
            "echo" => {
                // Deserialize request
                let request = EchoRequest::decode(payload)
                    .map_err(|e| Error::Decoding(e))?;
                
                // Create response
                let response = EchoResponse {
                    message: format!("Echo: {}", request.message),
                };
                
                // Serialize response
                let mut buf = BytesMut::with_capacity(response.encoded_len());
                response.encode(&mut buf)
                    .map_err(|e| Error::Encoding(e))?;
                
                Ok(buf.freeze())
            }
            "stream" => {
                // This is just an example implementation since bidirectional streaming
                // would be implemented differently - here we just return the first response
                let request = StreamRequest::decode(payload)
                    .map_err(|e| Error::Decoding(e))?;
                
                // Create first response in the stream
                let response = StreamResponse {
                    sequence: 1,
                    payload: format!("First response of {}", request.count),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                };
                
                // Serialize response
                let mut buf = BytesMut::with_capacity(response.encoded_len());
                response.encode(&mut buf)
                    .map_err(|e| Error::Encoding(e))?;
                
                Ok(buf.freeze())
            }
            _ => Err(Error::MethodNotFound(method.to_string())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["echo".into(), "stream".into()]
    }
}

// Heavy computation service example
struct ComputeService;

#[async_trait]
impl Service for ComputeService {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error> {
        #[derive(Serialize, Deserialize)]
        struct MatrixRequest {
            size: usize,
        }
        
        #[derive(Serialize, Deserialize)]
        struct MatrixResponse {
            result: Vec<Vec<f64>>,
            elapsed_ms: u64,
        }
        
        match method {
            "matrix_multiply" => {
                // Deserialize request
                let request: MatrixRequest = serde_json::from_slice(&payload)
                    .map_err(|e| Error::Deserialization(e))?;
                
                let size = request.size;
                if size > 1000 {
                    return Err(Error::InvalidConfig("Matrix size too large".into()));
                }
                
                // Simulate heavy computation with Rayon
                let start = std::time::Instant::now();
                
                // Generate random matrices
                let a = generate_random_matrix(size);
                let b = generate_random_matrix(size);
                
                // Perform matrix multiplication in parallel using Rayon
                let result = multiply_matrices(&a, &b);
                
                let elapsed = start.elapsed();
                
                // Create response
                let response = MatrixResponse {
                    result,
                    elapsed_ms: elapsed.as_millis() as u64,
                };
                
                // Serialize response
                let json = serde_json::to_vec(&response)
                    .map_err(|e| Error::Serialization(e))?;
                
                Ok(Bytes::from(json))
            }
            _ => Err(Error::MethodNotFound(method.to_string())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["matrix_multiply".into()]
    }
}

// Generate a random matrix of the given size
fn generate_random_matrix(size: usize) -> Vec<Vec<f64>> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    (0..size)
        .map(|_| (0..size).map(|_| rng.gen::<f64>()).collect())
        .collect()
}

// Multiply two matrices using Rayon for parallelism
fn multiply_matrices(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let size = a.len();
    let mut result = vec![vec![0.0; size]; size];
    
    rayon::scope(|s| {
        for i in 0..size {
            let (a, b, mut result) = (&a, &b, &mut result);
            s.spawn(move |_| {
                for j in 0..size {
                    let mut sum = 0.0;
                    for k in 0..size {
                        sum += a[i][k] * b[k][j];
                    }
                    result[i][j] = sum;
                }
            });
        }
    });
    
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();
    
    // Load or generate certificates
    let cert_path = "certs/server.crt";
    let key_path = "certs/server.key";
    
    if !std::path::Path::new(cert_path).exists() || !std::path::Path::new(key_path).exists() {
        // Create directory if it doesn't exist
        std::fs::create_dir_all("certs").context("Failed to create certs directory")?;
        
        // Generate self-signed certificate for testing
        info!("Generating self-signed certificate for testing...");
        generate_self_signed_cert("certs/server")?;
    }
    
    // Create server configuration
    let config = Config {
        addr: "127.0.0.1:4433".parse().unwrap(),
        cert_path: Some(cert_path.into()),
        key_path: Some(key_path.into()),
        format: quicserve::SerializationFormat::Protobuf,
        ..Default::default()
    };
    
    // Create and start server
    let server = Server::new(config).await?;
    
    // Register services
    server.register_service("echo", EchoService).await?;
    server.register_service("compute", ComputeService).await?;
    
    info!("Server started. Press Ctrl+C to quit.");
    
    // Start the server
    server.serve().await?;
    
    Ok(())
}

// Generate a self-signed certificate for testing
fn generate_self_signed_cert(prefix: &str) -> Result<()> {
    use rcgen::{Certificate, CertificateParams, KeyPair, KeyUsagePurpose, date_time_ymd};
    
    // Create certificate parameters
    let mut params = CertificateParams::default();
    params.not_before = date_time_ymd(2023, 1, 1);
    params.not_after = date_time_ymd(2030, 1, 1);
    params.distinguished_name.push(rcgen::DnType::CommonName, "localhost");
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    
    // Generate certificate
    let cert = Certificate::from_params(params)?;
    
    // Save certificate and private key
    std::fs::write(format!("{}.crt", prefix), cert.serialize_pem()?)?;
    std::fs::write(format!("{}.key", prefix), cert.serialize_private_key_pem())?;
    
    Ok(())
}