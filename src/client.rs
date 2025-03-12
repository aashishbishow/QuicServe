use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use h3::quic::Connection;
use h3_webtransport::client;
use log::{debug, error, info, warn};
use quinn::{ClientConfig, Endpoint};
use tokio::sync::{mpsc, Mutex, RwLock, oneshot};

use crate::{config::Config, error::Error, Request, Response, WEBTRANSPORT_PROTOCOL};
use crate::transport::MessageStream;

/// Type definition for RPC response channels
type ResponseChannel = oneshot::Sender<Result<Bytes, Error>>;

/// RPC Client implementation
pub struct Client {
    /// Configuration
    config: Config,
    /// QUIC endpoint
    endpoint: Endpoint,
    /// WebTransport session
    session: Arc<Mutex<Option<client::Session>>>,
    /// Message stream for communication
    message_stream: Arc<Mutex<Option<MessageStream>>>,
    /// Pending requests waiting for responses
    pending: Arc<Mutex<HashMap<u64, ResponseChannel>>>,
    /// Next request ID
    next_id: Arc<Mutex<u64>>,
}

impl Client {
    /// Creates a new Client instance
    pub async fn new(config: Config) -> Result<Self, Error> {
        // Build client configuration
        let client_config = config.build_client_config()?;
        
        // Create QUIC endpoint
        let mut endpoint = Endpoint::client("[::]:0".parse().unwrap())
            .map_err(|e| Error::Quic(format!("Failed to create endpoint: {}", e)))?;
        
        // Set ALPN protocols for HTTP/3
        endpoint.set_default_client_config(client_config);
        
        Ok(Self {
            config,
            endpoint,
            session: Arc::new(Mutex::new(None)),
            message_stream: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
        })
    }
    
    /// Connects to the RPC server
    pub async fn connect(&self) -> Result<(), Error> {
        // Connect to the server
        let connection = self.endpoint.connect(self.config.addr, 
            self.config.server_name.as_deref().unwrap_or("localhost"))
            .map_err(|e| Error::Quic(format!("Failed to connect: {}", e)))?
            .await
            .map_err(|e| Error::Quic(format!("Connection failed: {}", e)))?;
            
        info!("Connected to {}", self.config.addr);
        
        // Create HTTP/3 connection
        let h3_conn = h3::client::Connection::new(h3::quic::Connection::new(connection))
            .await
            .map_err(|e| Error::Http3(format!("Failed to create HTTP/3 connection: {}", e)))?;
        
        // Create WebTransport session
        let session = client::Builder::new()
            .enable_webtransport(true)
            .enable_datagram(true)
            .build(h3_conn)
            .await
            .map_err(|e| Error::WebTransport(format!("Failed to create WebTransport client: {}", e)))?;
        
        // Connect to the RPC endpoint
        let session = session.connect("/rpc")
            .await
            .map_err(|e| Error::WebTransport(format!("Failed to connect to RPC endpoint: {}", e)))?;
            
        debug!("WebTransport session established");
        
        // Open bidirectional stream for RPC communication
        let stream = session.open_bi()
            .await
            .map_err(|e| Error::WebTransport(format!("Failed to open bidirectional stream: {}", e)))?;
            
        debug!("Bidirectional stream opened");
        
        // Create message stream wrapper
        let message_stream = MessageStream::new(stream);
        
        // Update client state
        {
            let mut session_guard = self.session.lock().await;
            *session_guard = Some(session);
        }
        {
            let mut stream_guard = self.message_stream.lock().await;
            *stream_guard = Some(message_stream);
        }
        
        // Start response handler
        self.start_response_handler().await;
        
        Ok(())
    }
    
    /// Starts the response handler to process incoming messages
    async fn start_response_handler(&self) {
        let message_stream = self.message_stream.clone();
        let pending = self.pending.clone();
        let format = self.config.format;
        
        tokio::spawn(async move {
            let mut stream = {
                let mut guard = message_stream.lock().await;
                match guard.take() {
                    Some(stream) => stream,
                    None => {
                        error!("Message stream not initialized");
                        return;
                    }
                }
            };
            
            // Process incoming responses
            while let Some(response_bytes) = match stream.receive().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    error!("Error receiving response: {}", e);
                    break;
                }
            } {
                // Deserialize response
                let response: Response = match crate::deserialize(&response_bytes, format) {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("Failed to deserialize response: {}", e);
                        continue;
                    }
                };
                
                debug!("Received response for request {}", response.id);
                
                // Find corresponding pending request
                let sender = {
                    let mut pending_guard = pending.lock().await;
                    pending_guard.remove(&response.id)
                };
                
                // Send response to waiting caller
                if let Some(sender) = sender {
                    let result = match response.error {
                        Some(err) => Err(Error::RpcFailed(err)),
                        None => Ok(response.payload.unwrap_or_else(|| Bytes::new())),
                    };
                    
                    if sender.send(result).is_err() {
                        debug!("Failed to send response to caller - caller dropped");
                    }
                } else {
                    debug!("No pending request found for response ID: {}", response.id);
                }
            }
            
            // Put the stream back for reconnection handling
            let mut guard = message_stream.lock().await;
            *guard = Some(stream);
            
            debug!("Response handler exited");
        });
    }
    
    /// Calls a remote procedure and returns the result
    pub async fn call<T, R>(&self, method: &str, request: &T) -> Result<R, Error>
    where
        T: serde::Serialize + prost::Message,
        R: serde::de::DeserializeOwned + prost::Message + Default,
    {
        // Serialize request payload
        let payload = crate::serialize(request, self.config.format)?;
        
        // Get next request ID
        let id = {
            let mut id_guard = self.next_id.lock().await;
            let id = *id_guard;
            *id_guard = id.wrapping_add(1);
            id
        };
        
        // Create RPC request
        let rpc_request = Request {
            id,
            method: method.to_string(),
            payload,
        };
        
        // Create response channel
        let (tx, rx) = oneshot::channel();
        
        // Register pending request
        {
            let mut pending_guard = self.pending.lock().await;
            pending_guard.insert(id, tx);
        }
        
        // Serialize and send request
        let request_bytes = crate::serialize(&rpc_request, self.config.format)?;
        {
            let mut stream_guard = self.message_stream.lock().await;
            let stream = stream_guard.as_mut()
                .ok_or_else(|| Error::ConnectionClosed)?;
            
                stream.send(request_bytes).await
                .map_err(|e| Error::WebTransport(format!("Failed to send request: {}", e)))?;
        }

        // Wait for response with timeout
        let response_bytes = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            rx,
        ).await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::ConnectionClosed)??;
        
        // Deserialize response
        let result = crate::deserialize(&response_bytes, self.config.format)?;
        Ok(result)
    }
    
    /// Closes the connection to the server
    pub async fn close(&self) -> Result<(), Error> {
        // Close session if open
        let mut session_guard = self.session.lock().await;
        if let Some(session) = session_guard.take() {
            debug!("Closing WebTransport session");
            session.close().await;
        }
        
        // Close message stream
        let mut stream_guard = self.message_stream.lock().await;
        *stream_guard = None;
        
        // Clear pending requests with errors
        let mut pending_guard = self.pending.lock().await;
        for (_, sender) in pending_guard.drain() {
            let _ = sender.send(Err(Error::ConnectionClosed));
        }
        
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Close the endpoint to prevent resource leaks
        self.endpoint.close(0u32.into(), &[]);
    }
}