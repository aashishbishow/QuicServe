use std::{fs::File, io::BufReader, sync::Arc};
use tokio::net::UdpSocket;
use tokio::task;
use quinn::{Endpoint, ServerConfig, Incoming};
use rustls::{Certificate, PrivateKey};
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (cert, key) = load_certs("cert.pem", "key.pem")?;
    let server_config = configure_server(cert, key)?;

    let mut endpoint = Endpoint::server(server_config, "[::]:4433".parse()?)?;
    println!("Server running on https://localhost:4433");

    while let Some(conn) = endpoint.accept().await {
        task::spawn(handle_connection(conn));
    }

    Ok(())
}

async fn handle_connection(conn: quinn::Connecting) {
    let conn = match conn.await {
        Ok(c) => c,
        Err(_) => return,
    };

    while let Some(stream) = conn.accept_bi().await.ok() {
        let (mut send, recv) = stream;
        let response = b"HTTP/3 Server: Hello, World!";
        let _ = send.write_all(response).await;
        let _ = send.finish().await;
    }
}

fn load_certs(cert_path: &str, key_path: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    let cert_file = File::open(cert_path)?;
    let mut reader = BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut reader)?;
    let key_file = File::open(key_path)?;
    let mut reader = BufReader::new(key_file);
    let mut keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
    
    if keys.is_empty() {
        return Err("No keys found".into());
    }

    Ok((certs[0].clone(), keys.remove(0)))
}

fn configure_server(cert: Vec<u8>, key: Vec<u8>) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let mut config = ServerConfig::with_single_cert(vec![Certificate(cert)], PrivateKey(key))?;
    config.transport_config_mut().keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    Ok(config)
}
