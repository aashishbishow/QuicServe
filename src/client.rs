use quinn::{ClientConfig, Endpoint};
use rustls::{Certificate, ClientConfig as RustlsConfig, PrivateKey, RootCertStore};
use std::{fs, net::SocketAddr, sync::Arc};
use tonic::Request;
use tonic::transport::Channel;
use hello::greeter_client::GreeterClient;
use hello::HelloRequest;

pub mod hello {
    tonic::include_proto!("hello");
}

// Load TLS configuration
fn configure_tls(cert_path: &str, key_path: &str) -> ClientConfig {
    let cert = fs::read(cert_path).expect("Failed to read certificate");
    let key = fs::read(key_path).expect("Failed to read private key");

    let cert = Certificate(cert);
    let key = PrivateKey(key);

    let mut roots = RootCertStore::empty();
    roots.add(&cert).unwrap();

    let tls_config = RustlsConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_client_auth_cert(vec![cert], key)
        .unwrap();

    let tls_config = Arc::new(tls_config);
    ClientConfig::new(tls_config)
}

// Establish QUIC connection
async fn connect_quic(addr: &str, cert_path: &str, key_path: &str) -> Endpoint {
    let addr: SocketAddr = addr.parse().unwrap();
    let tls_config = configure_tls(cert_path, key_path);

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(tls_config);

    println!("🔗 Connecting to QUIC server: {}", addr);
    let conn = endpoint.connect(addr, "localhost").unwrap().await.unwrap();
    println!("✅ QUIC connection established!");

    endpoint
}

// gRPC call over HTTP/3
async fn grpc_call(channel: Channel) {
    let mut client = GreeterClient::new(channel);

    let request = Request::new(HelloRequest {
        name: "QUIC Client".to_string(),
    });

    match client.say_hello(request).await {
        Ok(response) => println!("📬 Response: {:?}", response.into_inner()),
        Err(e) => eprintln!("❌ gRPC Error: {:?}", e),
    }
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:4433";
    let cert_path = "certs/client_cert.pem";
    let key_path = "certs/client_key.pem";

    let endpoint = connect_quic(addr, cert_path, key_path).await;

    // Use gRPC over QUIC connection
    let channel = Channel::from_static("http://localhost:50051")
        .connect()
        .await
        .unwrap();

    grpc_call(channel).await;
}
