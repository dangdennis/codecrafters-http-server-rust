use std::io::Write;
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut tcp_stream) => {
                println!("accepted new connection");

                tcp_stream
                    .write(b"HTTP/1.1 200 OK\r\n\r\n")
                    .expect("failed to write to stream");

                ()
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
