use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::str::Utf8Error;

fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").context("Failed to bind to address")?;

    for stream in listener.incoming() {
        let mut tcp_stream = stream.context("Failed to accept connection")?;
        println!("accepted new connection");

        handle_connection(&mut tcp_stream).context("Failed to handle connection")?;
    }

    Ok(())
}

fn handle_connection(tcp_stream: &mut std::net::TcpStream) -> Result<()> {
    let mut reader = BufReader::new(tcp_stream.by_ref());
    let mut buffer = Vec::new();

    // Read headers
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).context("Failed to read line")?;
        if line == "\r\n" {
            break;
        }
        buffer.extend_from_slice(line.as_bytes());
    }

    let response = match HttpServer::parse_request(&buffer) {
        Ok(request) => match request.path {
            "/" => "HTTP/1.1 200 OK\r\n\r\n",
            _ => "HTTP/1.1 404 Not Found\r\n\r\n",
        },
        Err(e) => {
            println!("failed to parse request: {:?}", e);
            "HTTP/1.1 404 Not Found\r\n\r\n"
        }
    };

    tcp_stream
        .write_all(response.as_bytes())
        .context("Failed to write response")?;

    Ok(())
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

#[derive(Debug)]
struct Request<'a> {
    method: Method,
    path: &'a str,
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

#[derive(Debug)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

#[derive(Debug)]
enum Version {
    Http1_0,
    Http1_1,
    Http2_0,
}
