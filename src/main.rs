use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::str::Utf8Error;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut tcp_stream) => {
                println!("accepted new connection");

                let mut buffer = Vec::new();
                match tcp_stream.read_to_end(&mut buffer) {
                    Ok(_) => {
                        let request = HttpServer::parse_request(&buffer).unwrap();

                        match request.path {
                            "http://localhost:4221/abcdefg" => {
                                tcp_stream
                                    .write(b"HTTP/1.1 404 Not Found\r\n\r\n")
                                    .expect("failed to write to stream");
                            }
                            "http://localhost:4221" => {
                                tcp_stream
                                    .write(b"HTTP/1.1 200 OK\r\n\r\n")
                                    .expect("failed to write to stream");
                            }
                        }

                        tcp_stream
                            .write(b"HTTP/1.1 200 OK\r\n\r\n")
                            .expect("failed to write to stream");

                        ()
                    }
                    Err(e) => {
                        println!("failed to read from stream: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

struct HttpServer {}

impl HttpServer {
    fn new() -> Self {
        Self {}
    }

    fn parse_request(input: &[u8]) -> Result<Request, ParseError> {
        let req_str = std::str::from_utf8(input)?;
        let mut lines = req_str.lines();
        let req_line = lines.next().ok_or(ParseError::InvalidRequest)?;
        let mut parts = req_line.split_whitespace();

        let method = parts.next().ok_or(ParseError::InvalidRequest)?;
        let method = Self::parse_method(method)?;

        let path = parts.next().ok_or(ParseError::InvalidRequest)?;

        let version = parts.next().ok_or(ParseError::InvalidRequest)?;
        let version = Self::parse_version(version)?;

        Ok(Request {
            method,
            path,
            version,
        })
    }

    fn parse_method(method: &str) -> Result<Method, ParseError> {
        match method {
            "GET" => Ok(Method::Get),
            "POST" => Ok(Method::Post),
            "PUT" => Ok(Method::Put),
            "DELETE" => Ok(Method::Delete),
            "PATCH" => Ok(Method::Patch),
            _ => Err(ParseError::InvalidMethod),
        }
    }

    fn parse_version(version: &str) -> Result<Version, ParseError> {
        match version {
            "HTTP/1.0" => Ok(Version::Http1_0),
            "HTTP/1.1" => Ok(Version::Http1_1),
            "HTTP/2.0" => Ok(Version::Http2_0),
            _ => Err(ParseError::InvalidVersion),
        }
    }
}

struct Request {
    method: Method,
    path: &str,
    version: Version,
}

#[derive(Debug)]
enum ParseError {
    InvalidRequest,
    InvalidMethod,
    InvalidVersion,
    Utf8Error(Utf8Error),
}

impl From<Utf8Error> for ParseError {
    fn from(err: Utf8Error) -> Self {
        ParseError::Utf8Error(err)
    }
}

enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

enum Version {
    Http1_0,
    Http1_1,
    Http2_0,
}
