use grpc_http3::grpc::service::create_service;
use grpc_http3::quic::transport::QuicTransport;
use tonic::transport::Server;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting gRPC over HTTP/3 over QUIC with Rayon");

    // QUIC transport
    let quic = QuicTransport::new();
    task::spawn_blocking(move || quic.start("0.0.0.0:4433"));

    // gRPC Service
    Server::builder()
        .add_service(create_service())
        .serve("[::1]:50051".parse()?)
        .await?;

    Ok(())
}
