use quiche::{Config, Connection, Http3Config};
use rayon::prelude::*;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

pub struct QuicTransport {
    config: Config,
    h3_config: Http3Config,
}

impl QuicTransport {
    pub fn new() -> Self {
        let mut config = Config::new(quiche::PROTOCOL_VERSION).unwrap();
        config
            .set_application_protos(b"\x05h3-29")
            .unwrap(); // HTTP/3 support
        config.set_initial_max_data(10_000_000);
        config.verify_peer(false);

        let h3_config = Http3Config::new().unwrap();

        Self { config, h3_config }
    }

    pub fn start(&self, addr: &str) {
        let socket = UdpSocket::bind(addr).expect("Failed to bind UDP socket");
        println!("🚀 Listening on {}", socket.local_addr().unwrap());

        let buf = Arc::new(Mutex::new([0; 65535]));

        // Main loop to receive packets and handle concurrently
        loop {
            let buf_clone = Arc::clone(&buf);

            // Spawn parallel workers with Rayon for packet handling
            (0..4).into_par_iter().for_each(|_| {
                let mut local_buf = buf_clone.lock().unwrap();
                if let Ok((len, peer_addr)) = socket.recv_from(&mut *local_buf) {
                    if let Ok(mut conn) = quiche::accept(&local_buf[..len], None, &self.config) {
                        println!("🎉 New QUIC connection from: {}", peer_addr);
                        self.handle_quic_stream(&mut conn);
                    }
                }
            });
        }
    }

    fn handle_quic_stream(&self, conn: &mut Connection) {
        if let Ok(mut h3_conn) = quiche::h3::Connection::with_transport(conn, &self.h3_config) {
            (0..4).into_par_iter().for_each(|_| {
                while let Some((stream_id, event)) = h3_conn.poll(conn).unwrap() {
                    if let quiche::h3::Event::Headers { list, .. } = event {
                        println!("📥 Received headers: {:?}", list);
                    }
                }
            });
        }
    }
}
