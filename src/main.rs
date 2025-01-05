use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use flate2::write::GzEncoder;
use flate2::Compression;

fn main() -> Result<(), ()> {
    let args: Vec<String> = std::env::args().collect();

    let directory = args
        .iter()
        .position(|arg| arg == "--directory")
        .and_then(|pos| args.get(pos + 1));

    let http_server = CodeCraftsHttpServer::new(directory);

    http_server.start()
}

struct CodeCraftsHttpServer {
    server: Arc<HttpServer>,
}

impl CodeCraftsHttpServer {
    fn new(file_dir: Option<&String>) -> Self {
        Self {
            server: Arc::new(HttpServer {
                file_dir: file_dir.cloned(),
            }),
        }
    }

    fn start(self) -> Result<(), ()> {
        let listener = TcpListener::bind("127.0.0.1:4221")
            .context("Failed to bind to address")
            .map_err(|err| {
                eprintln!("{:?}", err);
                ()
            })?;

        for stream in listener.incoming() {
            match stream {
                Ok(tcp_stream) => {
                    let server = Arc::clone(&self.server);
                    thread::spawn(move || {
                        if let Err(err) = server
                            .handle_connection(tcp_stream)
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
}

struct HttpServer {
    file_dir: Option<String>,
}

impl HttpServer {
    fn handle_connection(&self, mut tcp_stream: std::net::TcpStream) -> Result<()> {
        let mut reader = BufReader::new(&tcp_stream);
        let mut request_lines = Vec::new();
        let mut content_length = 0;

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).context("Failed to read line")?;
            if line.trim().is_empty() {
                break;
            }
            if line.starts_with("Content-Length:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    content_length = parts[1].trim().parse().context("Invalid Content-Length")?;
                }
            }
            request_lines.extend_from_slice(line.as_bytes());
        }

        if content_length > 0 {
            let mut body = vec![0; content_length];
            reader
                .read_exact(&mut body)
                .context("Failed to read body")?;
            request_lines.extend_from_slice(&body);
        }

        let response: Response = match HttpServer::parse_request(std::str::from_utf8(&request_lines)?) {
            Ok(request) => {
                let supports_gzip = request.headers.get("accept-encoding")
                    .map(|encodings| encodings.to_lowercase().contains("gzip"))
                    .unwrap_or(false);

                let path_vec = request.path.split('/').collect::<Vec<&str>>();
                let path_parts = path_vec.as_slice();
                match path_parts {
                    ["", ""] => self.handle_root_request(&request),
                    ["", "user-agent"] => self.handle_user_agent_request(&request),
                    ["", "echo", echo_str] => {
                        let mut resp = self.handle_echo_request(&request, echo_str);
                        if supports_gzip {
                            resp.should_compress = true;
                        }
                        resp
                    },
                    ["", "files", filename] => {
                        let mut resp = self.handle_file_request(&request, filename);
                        if supports_gzip {
                            resp.should_compress = true;
                        }
                        resp
                    },
                    _ => self.handle_not_found(&request),
                }
            }
            Err(e) => {
                println!("failed to parse request: {:?}", e);
                self.handle_internal_server_error()
            }
        };

        tcp_stream
            .write_all(response.to_http_string().as_bytes())
            .context("Failed to write response")?;

        Ok(())
    }

    fn handle_root_request(&self, request: &Request) -> Response {
        Response {
            body: None,
            headers: HashMap::new(),
            status_code: 200,
            version: request.version,
            should_compress: false,
        }
    }

    fn handle_user_agent_request(&self, request: &Request) -> Response {
        let ua = request.headers.get("user-agent").cloned();
        let mut resp_headers = HashMap::new();
        resp_headers.insert("Content-Type".into(), "text/plain".into());
        resp_headers.insert(
            "Content-Length".into(),
            ua.as_ref()
                .map_or("0".to_string(), |ua_str| ua_str.len().to_string()),
        );

        Response {
            status_code: 200,
            version: request.version,
            headers: resp_headers,
            body: ua,
            should_compress: false,
        }
    }

    fn handle_echo_request(&self, request: &Request, echo_str: &str) -> Response {
        let body = echo_str.to_string();
        let mut resp_headers = HashMap::new();
        resp_headers.insert("Content-Type".into(), "text/plain".into());
        resp_headers.insert("Content-Length".into(), body.len().to_string());

        Response {
            status_code: 200,
            version: request.version,
            headers: resp_headers,
            body: Some(body),
            should_compress: false,
        }
    }

    fn handle_file_request(&self, request: &Request, filename: &str) -> Response {
        println!("handling request {:?}", request);
        if let Some(file_dir) = &self.file_dir {
            let file_path = format!("{}/{}", file_dir, filename);

            let mut resp_headers = HashMap::<String, String>::new();
            resp_headers.insert("Content-Type".into(), "application/octet-stream".into());

            match request.method {
                Method::Get => {
                    let file_content = match File::open(&file_path) {
                        Ok(mut file) => {
                            let mut content = String::new();
                            file.read_to_string(&mut content)
                                .context("Failed to read file")
                                .ok()
                                .map(|_| content)
                        }
                        Err(_) => None,
                    };

                    if let Some(body) = file_content {
                        resp_headers.insert("Content-Length".into(), body.len().to_string());
                        Response {
                            status_code: 200,
                            version: request.version,
                            headers: resp_headers,
                            body: Some(body),
                            should_compress: false,
                        }
                    } else {
                        Response {
                            status_code: 404,
                            version: request.version,
                            headers: HashMap::new(),
                            body: None,
                            should_compress: false,
                        }
                    }
                }
                Method::Post => {
                    if let Some(body) = &request.body {
                        if let Ok(mut file) =
                            File::create(file_path).context("Failed to create file")
                        {
                            if let Ok(_) = file
                                .write_all(body.as_bytes())
                                .context("Failed to write to file")
                            {
                                Response {
                                    status_code: 201,
                                    version: request.version,
                                    headers: resp_headers,
                                    body: Some(body.clone()),
                                    should_compress: false,
                                }
                            } else {
                                eprintln!("failed to write file");
                                Response {
                                    status_code: 500,
                                    version: request.version,
                                    headers: HashMap::new(),
                                    body: None,
                                    should_compress: false,
                                }
                            }
                        } else {
                            eprintln!("failed to create file");
                            Response {
                                status_code: 500,
                                version: request.version,
                                headers: HashMap::new(),
                                body: None,
                                should_compress: false,
                            }
                        }
                    } else {
                        Response {
                            status_code: 500,
                            version: request.version,
                            headers: HashMap::new(),
                            body: None,
                            should_compress: false,
                        }
                    }
                }
                _ => {
                    eprintln!("unhandled request method");
                    Response {
                        body: None,
                        status_code: 500,
                        version: request.version,
                        headers: HashMap::new(),
                        should_compress: false,
                    }
                }
            }
        } else {
            Response {
                status_code: 404,
                version: request.version,
                headers: HashMap::new(),
                body: None,
                should_compress: false,
            }
        }
    }

    fn handle_not_found(&self, request: &Request) -> Response {
        Response {
            status_code: 404,
            version: request.version,
            headers: HashMap::new(),
            body: None,
            should_compress: false,
        }
    }

    fn handle_internal_server_error(&self) -> Response {
        Response {
            status_code: 500,
            version: Version::Http1_1,
            headers: HashMap::new(),
            body: None,
            should_compress: false,
        }
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
            if let Some((key, value)) = line.split_once(": ") {
                headers.insert(key.to_lowercase().to_string(), value.to_string());
            } else {
                // Lazy me assuming the last line is the body.
                body = Some(line.to_string());
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
    should_compress: bool,
}

impl Response {
    fn to_http_string(&self) -> String {
        let mut headers = self.headers.clone();
        let body = if self.should_compress && self.body.is_some() {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            if let Some(body) = &self.body {
                encoder.write_all(body.as_bytes()).unwrap_or_default();
                if let Ok(compressed) = encoder.finish() {
                    headers.insert("Content-Encoding".to_string(), "gzip".to_string());
                    headers.insert("Content-Length".to_string(), compressed.len().to_string());
                    String::from_utf8_lossy(&compressed).to_string()
                } else {
                    self.body.clone().unwrap_or_default()
                }
            } else {
                String::new()
            }
        } else {
            self.body.clone().unwrap_or_default()
        };

        let status_line = format!(
            "{} {} {}",
            self.version.to_str(),
            self.status_code,
            self.reason_phrase()
        );
        let headers: String = headers
            .iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<String>>()
            .join("\r\n");

        format!(
            "{}\r\n{}\r\n\r\n{}",
            status_line,
            headers,
            body
        )
    }

    fn reason_phrase(&self) -> &str {
        match self.status_code {
            200 => "OK",
            201 => "Created",
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

#[derive(Debug, Clone, Copy)]
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
