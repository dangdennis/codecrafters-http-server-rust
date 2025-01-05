use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread::{self, Thread};

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
            .map_err(|err| {
                eprintln!("{:?}", err);
                ()
            })?;

        for stream in listener.incoming() {
            match stream {
                Ok(mut tcp_stream) => {
                    thread::spawn(move || {
                        let server = HttpServer::new();
                        if let Err(err) = server
                            .handle_connection(&mut tcp_stream)
                            .context("Failed to handle connection")
                        {
                            eprintln!("{:?}", err);
                        }
                    });
                }
                Err(err) => {
                    eprintln!("Failed to accept connection: {:?}", err);
                }
            }
        }

        Ok(())
    }

    fn parse_request(input: &str) -> Result<Request, ParseError> {
        let mut lines = input.lines().peekable();
        let req_line = lines.next().ok_or(ParseError::InvalidRequest)?;
        let mut parts = req_line.split_whitespace();

        let method = parts.next().ok_or(ParseError::InvalidRequest)?;
        let method = Self::parse_method(method)?;

        let path = parts.next().ok_or(ParseError::InvalidRequest)?;

        let version = parts.next().ok_or(ParseError::InvalidRequest)?;
        let version = Self::parse_version(version)?;

        let mut headers = HashMap::new();
        let mut body = None;

        while let Some(line) = lines.next() {
            if line.is_empty() {
                // Empty line indicates the end of headers and start of body
                body = Some(lines.collect::<Vec<&str>>().join("\n"));
                break;
            }

            if let Some((key, value)) = line.split_once(": ") {
                headers.insert(key.to_lowercase().to_string(), value.to_string());
            }
        }

        Ok(Request {
            method,
            path,
            version,
            headers,
            body,
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
        let mut request_lines = Vec::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).context("Failed to read line")?;
            if line == "\r\n" {
                break;
            }
            request_lines.extend_from_slice(line.as_bytes());
        }

        let request_lines = std::str::from_utf8(&request_lines)?;

        let response: String = match HttpServer::parse_request(request_lines) {
            Ok(request) => {
                let path_vec = request.path.split('/').collect::<Vec<&str>>();
                let path_parts = path_vec.as_slice();
                match path_parts {
                    ["", ""] => Response {
                        body: None,
                        headers: HashMap::new(),
                        status_code: 200,
                        version: request.version,
                    }
                    .to_http_string(),
                    ["", "user-agent"] => {
                        let ua = request.headers.get("user-agent").cloned();
                        let mut resp_headers = HashMap::new();
                        resp_headers.insert("Content-Type".into(), "text/plain".into());
                        resp_headers.insert(
                            "Content-Length".into(),
                            ua.as_ref()
                                .map_or("0".to_string(), |ua_str| ua_str.len().to_string()),
                        );

                        let res = Response {
                            status_code: 200,
                            version: request.version,
                            headers: resp_headers,
                            body: ua,
                        };

                        res.to_http_string()
                    }
                    ["", "echo", echo_str] => {
                        let body = (*echo_str).to_string();
                        let mut resp_headers = HashMap::new();
                        resp_headers.insert("Content-Type".into(), "text/plain".into());
                        resp_headers.insert("Content-Length".into(), body.len().to_string());

                        let res = Response {
                            status_code: 200,
                            version: request.version,
                            headers: resp_headers,
                            body: Some(body),
                        };

                        res.to_http_string()
                    }
                    _ => Response {
                        body: None,
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
                    body: None,
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
    headers: HashMap<String, String>,
    body: Option<String>,
}

#[derive(Debug)]
struct Response {
    status_code: u32,
    version: Version,
    headers: HashMap<String, String>,
    body: Option<String>,
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

        format!(
            "{}\r\n{}\r\n\r\n{}",
            status_line,
            headers,
            self.body.clone().unwrap_or_else(|| "".to_string())
        )
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
