use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use h3::quic::Connection;
use h3_webtransport::{server, session::AcceptRequest, Session};
use log::{debug, error, info, warn};
use quinn::{Endpoint, ServerConfig};
use tokio::sync::{Mutex, RwLock};

use crate::{config::Config, error::Error, Request, Response, Service, WEBTRANSPORT_PROTOCOL};
use crate::transport::MessageStream;

/// RPC Server implementation
pub struct Server {
    /// Configuration
    config: Config,
    /// QUIC endpoint
    endpoint: Endpoint,
    /// Registered services
    services: Arc<RwLock<HashMap<String, Arc<dyn Service>>>>,
}

impl Server {
    /// Creates a new Server instance
    pub async fn new(config: Config) -> Result<Self, Error> {
        // Build server configuration
        let server_config = config.build_server_config()?;
        
        // Create QUIC endpoint
        let mut endpoint = Endpoint::server(server_config, config.addr)
            .map_err(|e| Error::Quic(format!("Failed to create endpoint: {}", e)))?;
        
        // Set ALPN protocols for HTTP/3
        endpoint.set_protocols(&[WEBTRANSPORT_PROTOCOL.to_vec()]);
        
        Ok(Self {
            config,
            endpoint,
            services: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Registers a service with the server
    pub async fn register_service<S: Service>(&self, name: &str, service: S) -> Result<(), Error> {
        let mut services = self.services.write().await;
        services.insert(name.to_string(), Arc::new(service));
        Ok(())
    }
    
    /// Starts the server and begins accepting connections
    pub async fn serve(self) -> Result<(), Error> {
        info!("Server listening on {}", self.config.addr);
        
        let server = Arc::new(self);
        
        loop {
            // Accept new QUIC connections
            let connection = match server.endpoint.accept().await {
                Some(conn) => match conn.await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        continue;
                    }
                },
                None => {
                    error!("Endpoint closed");
                    break Ok(());
                }
            };
            
            info!("Accepted connection from {}", connection.remote_address());
            
            // Clone server reference for the new connection
            let server_clone = server.clone();
            
            // Spawn a new task to handle the connection
            tokio::spawn(async move {
                if let Err(e) = server_clone.handle_connection(connection).await {
                    error!("Connection error: {}", e);
                }
            });
        }
    }
    
    /// Handles a new QUIC connection
    async fn handle_connection(&self, connection: quinn::Connection) -> Result<(), Error> {
        debug!("New connection from {}", connection.remote_address());
        
        // Create HTTP/3 connection
        let h3_conn = h3::server::Connection::new(h3::quic::Connection::new(connection))
            .await
            .map_err(|e| Error::Http3(format!("Failed to create HTTP/3 connection: {}", e)))?;
        
        // Create WebTransport session acceptor
        let mut acceptor = server::Builder::new()
            .enable_webtransport(true)
            .enable_datagram(true)
            .enable_connect(true)
            .build(h3_conn)
            .await
            .map_err(|e| Error::WebTransport(format!("Failed to create WebTransport server: {}", e)))?;
        
        // Accept WebTransport sessions
        while let Some(accept_request) = acceptor.accept().await {
            let path = accept_request.request().uri().path().to_string();
            debug!("New session request to path: {}", path);
            
            if path == "/rpc" {
                // Accept the session
                match accept_request.accept().await {
                    Ok(session) => {
                        debug!("Session accepted");
                        let services = self.services.clone();
                        let config = self.config.clone();
                        
                        // Spawn a new task to handle the session
                        tokio::spawn(async move {
                            if let Err(e) = handle_session(session, services, config).await {
                                error!("Session error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept session: {}", e);
                    }
                }
            } else {
                // Reject sessions with unknown paths
                debug!("Rejecting session with unknown path: {}", path);
                accept_request.reject(h3::error::ErrorLevel::ConnectionError, h3::error::Code::RequestRejected).await
                    .map_err(|e| Error::WebTransport(format!("Failed to reject session: {}", e)))?;
            }
        }
        
        Ok(())
    }
}

/// Handles a WebTransport session
async fn handle_session(
    session: Session<server::Connection>,
    services: Arc<RwLock<HashMap<String, Arc<dyn Service>>>>,
    config: Config,
) -> Result<(), Error> {
    // Create a bidirectional stream for RPC communication
    let stream = match session.accept_bi().await {
        Ok(stream) => stream,
        Err(e) => {
            return Err(Error::WebTransport(format!("Failed to accept bidirectional stream: {}", e)));
        }
    };
    
    // Create message stream wrapper
    let mut message_stream = MessageStream::new(stream);
    
    // Process RPC requests
    while let Some(request_bytes) = message_stream.receive().await? {
        // Deserialize request
        let request: Request = crate::deserialize(&request_bytes, config.format)?;
        debug!("Received request: {} - method: {}", request.id, request.method);
        
        // Split method name into service and method parts
        let parts: Vec<&str> = request.method.splitn(2, '.').collect();
        if parts.len() != 2 {
            let error_response = Response {
                id: request.id,
                payload: None,
                error: Some(format!("Invalid method format. Expected 'service.method', got '{}'", request.method)),
            };
            
            // Serialize and send error response
            let response_bytes = crate::serialize(&error_response, config.format)?;
            message_stream.send(response_bytes).await?;
            continue;
        }
        
        let service_name = parts[0];
        let method_name = parts[1];
        
        // Look up service
        let services_read = services.read().await;
        let service = match services_read.get(service_name) {
            Some(service) => service.clone(),
            None => {
                let error_response = Response {
                    id: request.id,
                    payload: None,
                    error: Some(format!("Service not found: {}", service_name)),
                };
                
                // Serialize and send error response
                let response_bytes = crate::serialize(&error_response, config.format)?;
                message_stream.send(response_bytes).await?;
                continue;
            }
        };
        drop(services_read);
        
        // Create response future with timeout
        let timeout = tokio::time::timeout(
            std::time::Duration::from_millis(config.timeout_ms),
            service.call(method_name, request.payload),
        );
        
        // Execute service call with timeout
        let result = match timeout.await {
            Ok(result) => result,
            Err(_) => Err(Error::Timeout),
        };
        
        // Create response
        let response = match result {
            Ok(payload) => Response {
                id: request.id,
                payload: Some(payload),
                error: None,
            },
            Err(err) => Response {
                id: request.id,
                payload: None,
                error: Some(err.to_string()),
            },
        };
        
        // Serialize and send response
        let response_bytes = crate::serialize(&response, config.format)?;
        message_stream.send(response_bytes).await?;
    }
    
    Ok(())
}