// examples/client.rs
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use log::{debug, info, warn};
use prost::Message;
use serde::{Deserialize, Serialize};

use quicserve::{Client, Config};

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/quicserve.rs"));

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();
    
    // Create client configuration
    let config = Config {
        addr: "127.0.0.1:4433".parse().unwrap(),
        verify_peer: false, // Disable certificate verification for self-signed cert
        format: quicserve::SerializationFormat::Protobuf,
        ..Default::default()
    };
    
    // Create client
    let client = Client::new(config).await?;
    
    // Connect to server
    info!("Connecting to server...");
    client.connect().await?;
    info!("Connected successfully");
    
    // Call echo service
    info!("Calling echo service...");
    let request = EchoRequest {
        message: "Hello, QuicServe!".to_string(),
    };
    
    let response: EchoResponse = client.call("echo.echo", &request).await?;
    info!("Echo response: {}", response.message);
    
    // Call compute service
    info!("Calling compute service for matrix multiplication...");
    #[derive(Serialize, Deserialize)]
    struct MatrixRequest {
        size: usize,
    }
    
    #[derive(Serialize, Deserialize)]
    struct MatrixResponse {
        result: Vec<Vec<f64>>,
        elapsed_ms: u64,
    }
    
    let matrix_request = MatrixRequest { size: 100 };
    let matrix_response: MatrixResponse = client.call("compute.matrix_multiply", &matrix_request).await?;
    
    info!(
        "Matrix multiplication completed in {} ms, result matrix size: {}x{}",
        matrix_response.elapsed_ms,
        matrix_response.result.len(),
        matrix_response.result[0].len()
    );
    
    // Test performance with multiple parallel requests
    info!("Testing performance with parallel requests...");
    let start = std::time::Instant::now();
    let tasks: Vec<_> = (0..10).map(|i| {
        let client = client.clone();
        let request = EchoRequest {
            message: format!("Parallel request {}", i),
        };
        
        tokio::spawn(async move {
            let response: EchoResponse = client.call("echo.echo", &request).await?;
            Ok::<_, anyhow::Error>(response)
        })
    }).collect();
    
    for (i, task) in tasks.into_iter().enumerate() {
        let response = task.await??;
        debug!("Parallel response {}: {}", i, response.message);
    }
    
    let elapsed = start.elapsed();
    info!("Completed 10 parallel requests in {:?}", elapsed);
    
    // Close connection
    info!("Closing connection...");
    client.close().await?;
    
    Ok(())
}