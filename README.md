# QuicServe

**QuicServe** is a high-performance HTTP/3 web server built with Rust, leveraging Tokio, Quinn, and Hyper for fast, secure, and scalable web serving. It is ideal for developers looking to integrate HTTP/3 support into their projects or run a standalone QUIC-based server.

## Features

- **HTTP/3 Support**: Utilizes the QUIC protocol for reduced latency and improved performance.
- **Asynchronous I/O**: Built on Tokio for efficient, non-blocking operations.
- **Flexible Routing**: Integrates with Hyper to provide robust routing capabilities.
- **TLS Encryption**: Ensures secure data transmission with TLS support.

## Getting Started

### Prerequisites

- Rust (latest stable version)
- Cargo package manager

### Installation

Clone the repository:


```bash
git clone https://github.com/aashishbishow/QuicServe.git
```


Navigate to the project directory:


```bash
cd QuicServe
```


Build the project:


```bash
cargo build --release
```


### Usage

To run the server with the default configuration:


```bash
cargo run --release
```


For custom configurations, modify the `config.toml` file located in the project directory.

### Examples

The `examples` directory contains sample implementations demonstrating how to use QuicServe in various scenarios.

## Contributing

Contributions are welcome! Please fork the repository and create a pull request with your changes.

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.

## Acknowledgements

- [Tokio](https://tokio.rs/)
- [Quinn](https://github.com/quinn-rs/quinn)
- [Hyper](https://hyper.rs/)

For more information, visit the [QuicServe GitHub repository](https://github.com/aashishbishow/QuicServe). 
