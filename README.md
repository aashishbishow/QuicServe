## **QuikServe** 🚀  
**A high-performance, lightweight HTTP/3 server built with Rust, powered by Tokio, Quinn, and Hyper.**  

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)  
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org)  
[![Crates.io](https://img.shields.io/crates/v/quikserve.svg)](https://crates.io/crates/quikserve)  
[![Docs](https://docs.rs/quikserve/badge.svg)](https://docs.rs/quikserve)  

---

### **✨ Features**
✅ **Fully Async** – Uses `tokio` for non-blocking performance.  
✅ **HTTP/3 Support** – Uses `quinn` for QUIC-based communication.  
✅ **TLS by Default** – Secure communication with TLS (via `rustls`).  
✅ **Extensible** – Can be used as a **standalone server** or a **library** in Rust projects.  
✅ **Minimal and Fast** – Designed for speed and efficiency with low resource usage.  

---

## **📦 Installation**
You can install `quikserve` as a **binary** or use it as a **library** in your Rust projects.

### **1️⃣ Install CLI (Binary)**
```sh
cargo install quikserve
```
This will install the `quikserve` binary, allowing you to run a standalone HTTP/3 server.

### **2️⃣ Add as a Library**
If you want to embed QuikServe into your Rust project, add it to `Cargo.toml`:
```toml
[dependencies]
quikserve = "0.1"
```

---

## **🚀 Quick Start**
### **1️⃣ Generate TLS Certificates**
Since QUIC requires TLS, generate a self-signed certificate:
```sh
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes
```

### **2️⃣ Start the Server**
Run the HTTP/3 server on port `4433`:
```sh
quikserve --port 4433 --cert cert.pem --key key.pem
```
Now, visit **https://localhost:4433** in a QUIC-enabled browser (like Chrome or Firefox).  

---

## **📜 Usage**
### **CLI Options**
```sh
quikserve --help
```
| Flag | Description |
|------|------------|
| `--port <PORT>` | Specify the server port (default: 4433) |
| `--cert <FILE>` | Path to the TLS certificate file |
| `--key <FILE>` | Path to the TLS private key file |
| `--log-level <LEVEL>` | Set log verbosity (`info`, `debug`, `trace`) |

### **Using as a Library**
Create a simple HTTP/3 server in Rust:
```rust
use quikserve::Server;

#[tokio::main]
async fn main() {
    let server = Server::new("localhost:4433", "cert.pem", "key.pem")
        .expect("Failed to initialize server");
    server.run().await.expect("Server crashed");
}
```

---

## **🛠 Configuration**
QuikServe supports custom **TLS settings, request routing, and logging** via a configuration file (`config.toml`):

```toml
[server]
port = 4433
tls_cert = "cert.pem"
tls_key = "key.pem"

[logging]
level = "info"
```
Run the server with:
```sh
quikserve --config config.toml
```

---

## **⚡ Performance**
QuikServe is optimized for speed and low latency:  
✅ **Non-blocking I/O** via `tokio`  
✅ **Multiplexing** via `quinn`  
✅ **Zero-copy data transfer**  
✅ **Lightweight HTTP handler** with `hyper`  

---

## **🔒 Security**
- **TLS 1.3** enforced by `rustls`  
- **Automatic key rotation** (configurable)  
- **Rate limiting & request filtering** (planned)  

---

## **📖 Roadmap**
- [ ] Middleware support  
- [ ] WebSocket over HTTP/3  
- [ ] Load balancing & clustering  
- [ ] QUIC connection migration  

---

## **🛠 Contributing**
We welcome contributions! To get started:  
1. Fork the repo & clone locally.  
2. Run `cargo fmt` and `cargo clippy` before PRs.  
3. Open an issue for feature requests or bugs.  

---

## **📜 License**
QuikServe is licensed under the **MIT License**. See [LICENSE](LICENSE) for details.  

---

## **📞 Contact**
- **GitHub Issues**: [Report Bugs](https://github.com/aashishbishow/quikserve/issues)  
- **Discussions**: [Join Community](https://github.com/aashishbishow/quikserve/discussions)  

---
