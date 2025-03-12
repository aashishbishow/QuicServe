# QuicServe 🚀

**QuicServe** is a high-performance **Remote Procedure Call (RPC)** system built on top of **WebTransport** over **HTTP/3 QUIC**. With QuicServe, you can implement ultra-low-latency, scalable, and reliable communication for your distributed systems. This project uses state-of-the-art protocols like QUIC and WebTransport to deliver top-notch performance, supporting async runtimes, serialization, and easy cross-platform integrations.

## 🚀 Features

- **⚡ High-Performance RPC**: Enable efficient remote procedure calls using the latest transport protocols.
- **🌐 WebTransport over HTTP/3**: Built with WebTransport for seamless communication and enhanced speed over QUIC.
- **🔄 Fully Asynchronous**: Designed with `Tokio` to allow async processing for scalable, non-blocking operations.
- **🛠️ Cross-Platform**: Supports building extensions for various platforms like Python, Rust, WebAssembly, and more.
- **🖥️ Parallelism**: Leverage `rayon` for parallel execution and optimized CPU-bound tasks.
- **📦 Serialization Support**: Uses `serde` and `prost` for fast and flexible serialization (including Protocol Buffers).

## 🛠️ Installation

### Requirements
Make sure you have [Rust](https://www.rust-lang.org/learn/get-started) installed on your machine.

### Steps to Install

1. Clone the repository:

    ```bash
    git clone https://github.com/aashishbishowkarma/quicserve.git
    cd quicserve
    ```

2. Build and run:

    ```bash
    cargo build
    cargo run
    ```

### Application Usage

- **Asynchronous Remote Procedures**: Implement and invoke RPC calls asynchronously over a high-performance QUIC transport.
- **Cross-Language Integration**: Easily integrate with other programming languages like Python using `pyo3` or WebAssembly with `wasm-bindgen`.
- **HTTP/3 & WebTransport**: Take advantage of fast, reliable communication via HTTP/3 and WebTransport in distributed applications.

## 📚 Dependencies

QuicServe utilizes several powerful libraries to build its core features:

- **Networking**: 
  - `quinn` (QUIC transport)
  - `h3` (HTTP/3 protocol)
  - `h3-webtransport` (WebTransport over HTTP/3)
  
- **Serialization**:
  - `serde` for JSON and Rust structs
  - `serde_json` for efficient JSON parsing
  - `prost` for Protocol Buffers
  - `bytes` for efficient buffer management

- **Error Handling**:
  - `thiserror` for custom error types
  - `anyhow` for structured error handling

- **Asynchronous Programming**:
  - `tokio` for async runtimes and task scheduling
  - `rayon` for parallel processing
  
- **FFI (Foreign Function Interface)**:
  - `pyo3` to interface with Python
  - `wasm-bindgen` for WebAssembly integration
  - `uniffi` for cross-language bindings

- **Utilities**:
  - `log` for logging
  - `uuid` for unique identifiers

## 🔧 Configuration

- **Debug Profile**: Optimized for debugging.
  
  ```toml
  [profile.debug]
  opt-level = 2
  debug = true
  ```

- **Release Profile**: Optimized for performance.
  
  ```toml
  [profile.release]
  opt-level = 3
  ```

## 🎯 License

QuicServe is open-source software licensed under the **MIT License**. See the [LICENSE](LICENSE) file for more details.

## 🤝 Contributing

We welcome contributions! Whether you want to add a new feature, fix a bug, or improve the documentation, feel free to open an issue or submit a pull request. Make sure to follow the coding style and provide proper tests for your changes.

## 🌟 Keywords

- **RPC**: Remote Procedure Call
- **QUIC**: Fast and secure transport protocol
- **WebTransport**: Low-latency communication over HTTP/3
- **Async**: Non-blocking operations
- **Cross-Platform**: Build for multiple environments
- **Serialization**: Protocol Buffers, JSON
- **Networking**: QUIC, HTTP/3

## 👨‍💻 Author

- **Aashish BishowKarma**  
  Email: [aashishbishowkarma@outlook.com](mailto:aashishbishowkarma@outlook.com)

---

Thank you for checking out **QuicServe**! 🚀 We're excited to see how you use this fast and scalable RPC system in your own projects. Let us know your thoughts and feel free to contribute! ✨