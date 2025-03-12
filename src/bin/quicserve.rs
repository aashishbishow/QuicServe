use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};
use tokio::signal;

use quicserve::{
    client::Client,
    config::Config,
    error::Error,
    server::Server,
    utils::{configure_logging, parse_format, parse_socket_addr},
    SerializationFormat,
};

/// QuicServe: A high-performance RPC system using WebTransport over HTTP/3
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Enable debug logging
    #[clap(short, long)]
    debug: bool,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a QuicServe server
    Server {
        /// Address to bind to (e.g., "127.0.0.1:4433" or "[::1]:4433")
        #[clap(short, long, default_value = "[::1]:4433")]
        addr: String,

        /// TLS certificate file path (PEM format)
        #[clap(short, long)]
        cert: PathBuf,

        /// TLS private key file path (PEM format)
        #[clap(short, long)]
        key: PathBuf,

        /// Root CA certificate file path for client verification (optional)
        #[clap(long)]
        ca: Option<PathBuf>,

        /// Whether to verify client certificates
        #[clap(long)]
        verify_client: bool,

        /// Serialization format (json or protobuf)
        #[clap(short, long, default_value = "protobuf")]
        format: String,

        /// Maximum concurrent streams per connection
        #[clap(long, default_value = "100")]
        max_streams: u64,

        /// Keep-alive interval in milliseconds
        #[clap(long, default_value = "5000")]
        keep_alive: u64,

        /// Idle timeout in milliseconds
        #[clap(long, default_value = "30000")]
        idle_timeout: u64,
    },

    /// Connect to a QuicServe server
    Client {
        /// Server address to connect to (e.g., "127.0.0.1:4433" or "[::1]:4433")
        #[clap(short, long, default_value = "[::1]:4433")]
        addr: String,

        /// Server hostname for TLS verification
        #[clap(short, long, default_value = "localhost")]
        host: String,

        /// Root CA certificate file path for server verification (optional)
        #[clap(long)]
        ca: Option<PathBuf>,

        /// Serialization format (json or protobuf)
        #[clap(short, long, default_value = "protobuf")]
        format: String,

        /// RPC timeout in milliseconds
        #[clap(long, default_value = "30000")]
        timeout: u64,

        /// Keep-alive interval in milliseconds
        #[clap(long, default_value = "5000")]
        keep_alive: u64,

        /// Idle timeout in milliseconds
        #[clap(long, default_value = "30000")]
        idle_timeout: u64,

        /// Method to call (in format "service.method")
        #[clap(short, long)]
        method: Option<String>,

        /// Input data file path (for request payload)
        #[clap(short, long)]
        input: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Configure logging
    configure_logging();

    // Parse command line arguments
    let cli = Cli::parse();

    // Set log level based on debug flag
    if cli.debug {
        unsafe {
            std::env::set_var("RUST_LOG", "debug,quicserve=trace");
        }
        env_logger::try_init().ok();
    }

    // Handle commands
    match cli.command {
        Commands::Server {
            addr,
            cert,
            key,
            ca,
            verify_client,
            format,
            max_streams,
            keep_alive,
            idle_timeout,
        } => {
            run_server(
                addr,
                cert,
                key,
                ca,
                verify_client,
                format,
                max_streams,
                keep_alive,
                idle_timeout,
            )
            .await?;
        }
        Commands::Client {
            addr,
            host,
            ca,
            format,
            timeout,
            keep_alive,
            idle_timeout,
            method,
            input,
        } => {
            run_client(
                addr, host, ca, format, timeout, keep_alive, idle_timeout, method, input,
            )
            .await?;
        }
    }

    Ok(())
}

/// Run the QuicServe server
async fn run_server(
    addr: String,
    cert_path: PathBuf,
    key_path: PathBuf,
    ca_path: Option<PathBuf>,
    verify_peer: bool,
    format_str: String,
    max_concurrent_streams: u64,
    keep_alive_ms: u64,
    idle_timeout_ms: u64,
) -> Result<()> {
    // Parse address
    let addr = parse_socket_addr(&addr, 4433)
        .context("Failed to parse server address")?;

    // Parse serialization format
    let format = parse_format(&format_str)
        .context("Failed to parse serialization format")?;

    // Create server configuration
    let mut config = Config::new(addr);
    config.cert_path = Some(cert_path);
    config.key_path = Some(key_path);
    config.ca_path = ca_path;
    config.verify_peer = verify_peer;
    config.format = format;
    config.max_concurrent_streams = max_concurrent_streams;
    config.keep_alive_ms = Some(keep_alive_ms);
    config.idle_timeout_ms = Some(idle_timeout_ms);

    // Create and start server
    let server = Server::new(config).await?;
    
    // Register demo service (for example purposes)
    // In a real application, you would register your own services here
    // server.register_service("example", ExampleService::new()).await?;

    // Handle termination signals
    let server_task = tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            error!("Server error: {}", e);
        }
    });

    // Wait for Ctrl+C signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received, stopping server...");

    Ok(())
}

/// Run the QuicServe client
async fn run_client(
    addr: String,
    host: String,
    ca_path: Option<PathBuf>,
    format_str: String,
    timeout_ms: u64,
    keep_alive_ms: u64,
    idle_timeout_ms: u64,
    method: Option<String>,
    input: Option<PathBuf>,
) -> Result<()> {
    // Parse address
    let addr = parse_socket_addr(&addr, 4433)
        .context("Failed to parse server address")?;

    // Parse serialization format
    let format = parse_format(&format_str)
        .context("Failed to parse serialization format")?;

    // Create client configuration
    let mut config = Config::new(addr);
    config.ca_path = ca_path;
    config.server_name = Some(host);
    config.format = format;
    config.timeout_ms = timeout_ms;
    config.keep_alive_ms = Some(keep_alive_ms);
    config.idle_timeout_ms = Some(idle_timeout_ms);

    // Create client instance
    let client = Client::new(config).await?;

    // Connect to server
    client.connect().await?;
    info!("Connected to server at {}", addr);

    // If method specified, make an RPC call
    if let Some(method_name) = method {
        // Parse input data
        let input_data = match input {
            Some(path) => std::fs::read(&path).context("Failed to read input file")?,
            None => Vec::new(),
        };

        // Make RPC call (simplified example)
        info!("Calling method: {}", method_name);
        // In a real application, you would serialize your request type and deserialize the response
        // client.call::<YourRequestType, YourResponseType>(&method_name, &request).await?;
        
        // For demonstration, we just log that we would make the call
        info!("Would call {} with {} bytes of input data", method_name, input_data.len());
    } else {
        // Interactive mode or custom logic would go here
        info!("No method specified, enter interactive mode...");
        
        // For demonstration, we just wait for Ctrl+C
        signal::ctrl_c().await?;
    }

    // Close connection
    client.close().await?;
    info!("Connection closed");

    Ok(())
}