use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use prost::Message;
use bytes::{Bytes, BytesMut};
use tokio::runtime::Runtime;

use quicserve::{Client, Config, Server, Service, Error};

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/quicserve.rs"));

// Benchmark serialization and deserialization
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");
    
    // Create sample data
    let request = EchoRequest {
        message: "Hello, this is a benchmark message for serialization!".to_string(),
    };
    
    // Benchmark Protocol Buffers serialization
    group.bench_function("protobuf_serialize", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(request.encoded_len());
            request.encode(&mut buf).unwrap();
            black_box(buf.freeze())
        })
    });
    
    // Benchmark JSON serialization
    group.bench_function("json_serialize", |b| {
        b.iter(|| {
            let json = serde_json::to_vec(&request).unwrap();
            black_box(Bytes::from(json))
        })
    });
    
    // Create serialized data for deserialization benchmarks
    let mut protobuf_data = BytesMut::with_capacity(request.encoded_len());
    request.encode(&mut protobuf_data).unwrap();
    let protobuf_bytes = protobuf_data.freeze();
    
    let json_data = serde_json::to_vec(&request).unwrap();
    
    // Benchmark Protocol Buffers deserialization
    group.bench_function("protobuf_deserialize", |b| {
        b.iter(|| {
            black_box(EchoRequest::decode(protobuf_bytes.as_ref()).unwrap())
        })
    });
    
    // Benchmark JSON deserialization
    group.bench_function("json_deserialize", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<EchoRequest>(&json_data).unwrap())
        })
    });
    
    group.finish();
}

// Benchmark end-to-end RPC calls with different payload sizes
fn bench_rpc_calls(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Start server in the background
    let server_thread = std::thread::spawn(|| {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Create self-signed certificate
            let cert_path = "certs/bench_server.crt";
            let key_path = "certs/bench_server.key";
            
            if !std::path::Path::new(cert_path).exists() || !std::path::Path::new(key_path).exists() {
                std::fs::create_dir_all("certs").unwrap();
                generate_self_signed_cert("certs/bench_server").unwrap();
            }
            
            // Create server
            let config = Config {
                addr: "127.0.0.1:4434".parse().unwrap(),
                cert_path: Some(cert_path.into()),
                key_path: Some(key_path.into()),
                ..Default::default()
            };
            
            let server = Server::new(config).await.unwrap();
            server.register_service("bench", BenchService).await.unwrap();
            server.serve().await.unwrap();
        });
    });
    
    // Wait for server to start
    std::thread::sleep(std::time::Duration::from_secs(1));
    
    // Create client
    let client = rt.block_on(async {
        let config = Config {
            addr: "127.0.0.1:4434".parse().unwrap(),
            verify_peer: false,
            ..Default::default()
        };
        
        let client = Client::new(config).await.unwrap();
        client.connect().await.unwrap();
        client
    });
    
    let mut group = c.benchmark_group("rpc_calls");
    
    // Benchmark different payload sizes
    for size in [10, 100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let request = create_bench_request(size);
            
            b.iter(|| {
                rt.block_on(async {
                    let response: BenchResponse = client.call("bench.echo", &request).await.unwrap();
                    black_box(response)
                })
            });
        });
    }
    
    group.finish();
    
    // Close client
    rt.block_on(async {
        client.close().await.unwrap();
    });
}

// Create a bench request with a specific payload size
fn create_bench_request(size: usize) -> BenchRequest {
    let payload = vec![0u8; size];
    BenchRequest { payload }
}

// Custom message definitions for benchmarks
#[derive(Clone, Message)]
struct BenchRequest {
    #[prost(bytes, tag="1")]
    payload: Vec<u8>,
}

#[derive(Clone, Message)]
struct BenchResponse {
    #[prost(bytes, tag="1")]
    payload: Vec<u8>,
}

// Service implementation for benchmarks
struct BenchService;

#[async_trait::async_trait]
impl Service for BenchService {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error> {
        match method {
            "echo" => {
                // Echo the payload back
                Ok(payload)
            }
            _ => Err(Error::MethodNotFound(method.to_string())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["echo".into()]
    }
}

// Generate a self-signed certificate for testing
fn generate_self_signed_cert(prefix: &str) -> Result<(), Box<dyn std::error::Error>> {
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

criterion_group!(benches, bench_serialization, bench_rpc_calls);
criterion_main!(benches);