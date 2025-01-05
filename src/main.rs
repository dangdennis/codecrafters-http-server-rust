use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::str::Utf8Error;

fn main() -> Result<(), ()> {
    let http_server = HttpServer::new();
    http_server.listen()
}

struct HttpServer {}

impl HttpServer {
    fn new() -> Self {
        Self {}
    }

    fn listen(&self) -> Result<(), ()> {
        let listener = TcpListener::bind("127.0.0.1:4221")
            .context("Failed to bind to address")
            .map_err(|_err| ())?;

        for stream in listener.incoming() {
            let mut tcp_stream = stream
                .context("Failed to accept connection")
                .map_err(|_err| ())?;

            let _ = self
                .handle_connection(&mut tcp_stream)
                .context("Failed to handle connection");
        }

        Ok(())
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

    fn handle_connection(&self, tcp_stream: &mut std::net::TcpStream) -> Result<()> {
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

        let response: String = match HttpServer::parse_request(&buffer) {
            Ok(request) => {
                let path_vec = request.path.split('/').collect::<Vec<&str>>();
                let path_parts = path_vec.as_slice();
                match path_parts {
                    ["", ""] => {
                        println!("base root");
                        Response {
                            body: "".to_string(),
                            headers: HashMap::new(),
                            status_code: 200,
                            version: request.version,
                        }
                        .to_http_string()
                    }
                    ["", "echo", echo_str] => {
                        let body = (*echo_str).to_string();

                        let mut headers = HashMap::new();
                        headers.insert("Content-Type".into(), "text/plain".into());
                        headers.insert("Content-Length".into(), body.len().to_string());

                        let res = Response {
                            status_code: 200,
                            version: request.version,
                            headers,
                            body,
                        };

                        res.to_http_string()
                    }
                    _ => Response {
                        body: "".into(),
                        headers: HashMap::new(),
                        status_code: 404,
                        version: request.version,
                    }
                    .to_http_string(),
                }
            }
            Err(e) => {
                println!("failed to parse request: {:?}", e);
                Response {
                    body: "".into(),
                    headers: HashMap::new(),
                    status_code: 404,
                    version: Version::Http1_1,
                }
                .to_http_string()
            }
        };

        tcp_stream
            .write_all(response.as_bytes())
            .context("Failed to write response")?;

        Ok(())
    }
}

#[derive(Debug)]
struct Request<'a> {
    method: Method,
    path: &'a str,
    version: Version,
}

#[derive(Debug)]
struct Response {
    status_code: u32,
    version: Version,
    headers: HashMap<String, String>,
    body: String,
}

impl Response {
    fn to_http_string(&self) -> String {
        let status_line = format!(
            "{} {} {}",
            self.version.to_str(),
            self.status_code,
            self.reason_phrase()
        );
        let headers: String = self
            .headers
            .iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<String>>()
            .join("\r\n");

        format!("{}\r\n{}\r\n\r\n{}", status_line, headers, self.body)
    }

    fn reason_phrase(&self) -> &str {
        match self.status_code {
            200 => "OK",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown Status",
        }
    }
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

impl Version {
    fn to_str(&self) -> &str {
        match self {
            Version::Http1_0 => "HTTP/1.0",
            Version::Http1_1 => "HTTP/1.1",
            Version::Http2_0 => "HTTP/2.0",
        }
    }
}
