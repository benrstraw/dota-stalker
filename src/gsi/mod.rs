use std::collections::HashMap;
use bytes::BytesMut;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const OK: &str = "HTTP/1.1 200 OK\ncontent-type: text/html\n";

pub type JsonKV = HashMap<String, Value>;

pub struct Server {
	uri: String,
}

impl Server {
	pub fn new(uri: &str) -> Self {
		Server {
			uri: uri.to_owned(),
		}
	}

	pub async fn run(self, tx: mpsc::Sender<JsonKV>) {
		log::info!("Listening on {}", self.uri);

		let listener = TcpListener::bind(self.uri).await.unwrap(); // TODO: Handle.

		loop {
			let (mut socket, addr) = listener.accept().await.unwrap(); // TODO: Handle.
			log::info!("Accepted: {}", addr);
			let txi = tx.clone();

			let _ = tokio::spawn(async move {
				log::debug!("Task spawned...");

				if let Err(e) = socket.readable().await {
					log::error!("Socket not readable!");
					return; // TODO: Handle error.
				};

				let mut buf = BytesMut::with_capacity(122880);

				let n = match socket.read_buf(&mut buf).await {
					Ok(n) if n == 0 => {
						log::debug!("Socket closed!");
						return;
					}
					Ok(n) => n,
					Err(e) => {
						log::error!("Failed to read from socket!");
						return; // TODO: Handle error.
					}
				};

				log::trace!("Read: {}", n);

				if let Err(e) = socket.write_all(OK.as_bytes()).await {
					log::error!("Failed to write to socket!");
					return; // TODO: Handle error.
				};

				log::trace!("Raw request: {:?}", buf);
				let amt = match parse_headers(&buf) {
					Some(amt) => amt,
					None => {
						return; // TODO: Handle error, incomplete headers!
					}
				};

				let _ = buf.split_to(amt);
				log::trace!("Raw data: {:?}", buf);

				let game_data: JsonKV = serde_json::from_slice(&buf).expect("Failed to parse JSON body!");
				log::trace!("Parsed: {:?}", game_data);

				txi.send(game_data).await.unwrap();
			})
				.await.unwrap(); // TODO: Handle.
		}
	}
}

pub fn parse_headers(buf: &[u8]) -> Option<usize> {
	let mut headers = [httparse::EMPTY_HEADER; 16];
	let mut r = httparse::Request::new(&mut headers);

	let status = r.parse(buf).expect("Failed to parse HTTP request");

	match status {
		httparse::Status::Complete(amt) => Some(amt),
		httparse::Status::Partial => None,
	}
}
