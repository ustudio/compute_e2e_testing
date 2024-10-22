use anyhow;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::env;
use std::io::{BufRead, Cursor};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio;
use viceroy_lib::body::Body as ViceroyBody;
use viceroy_lib::{config, ExecuteCtx, ProfilingStrategy};

fn listen(
    rt: &tokio::runtime::Runtime,
    backend: fn(Request<Body>) -> Result<Response<Body>, Infallible>,
) -> (tokio::sync::oneshot::Sender<()>, u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let (addr_tx, addr_rx) = tokio::sync::oneshot::channel::<SocketAddr>();

    let make_svc = make_service_fn(move |_conn| async move {
        Ok::<_, Infallible>(service_fn(move |req| async move { backend(req) }))
    });

    rt.spawn(async move {
        let server = Server::bind(&addr).serve(make_svc);

        addr_tx
            .send(server.local_addr())
            .expect("server addr should be sent");

        let graceful = server.with_graceful_shutdown(async {
            stop_rx.await.ok();
        });

        if let Err(e) = graceful.await {
            eprintln!("Server failed!: {}", e);
        }
    });

    let listen_addr = addr_rx
        .blocking_recv()
        .expect("server addr should be received");

    return (stop_tx, listen_addr.port());
}

pub struct Requester {
    runtime: tokio::runtime::Runtime,
    backends: Vec<(String, tokio::sync::oneshot::Sender<()>, u16)>,
    dictionaries: HashMap<String, config::Dictionary>,
    captured_logs: Arc<Mutex<Vec<u8>>>,
}

impl Requester {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        Self {
            runtime,
            captured_logs: Arc::new(Mutex::new(Vec::new())),
            backends: Vec::new(),
            dictionaries: HashMap::new(),
        }
    }

    pub fn with_backend(
        mut self,
        name: &str,
        backend: fn(Request<Body>) -> Result<Response<Body>, Infallible>,
    ) -> Self {
        let (stop_server, port) = listen(&self.runtime, backend);

        self.backends.push((name.to_string(), stop_server, port));

        self
    }

    pub fn with_dictionary(mut self, name: &str, contents: &HashMap<&str, &str>) -> Self {
        self.dictionaries.insert(
            name.to_string(),
            config::Dictionary::InlineToml {
                contents: Arc::new(
                    contents
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect::<HashMap<String, String>>(),
                ),
            },
        );

        self
    }

    pub fn fetch(mut self, req: http::Request<Body>) -> HandledRequest {
        let ctx = ExecuteCtx::new(
            Path::new(
                env::var("CARGO_FASTLY_E2E_MODULE")
                    .expect("CARGO_FASTLY_E2E_MODULE environment variable should be set")
                    .as_str(),
            ),
            ProfilingStrategy::None,
            HashSet::new(),
            None,
            config::UnknownImportBehavior::LinkError,
            false,
        )
        .expect("Context should be created")
        .with_dictionaries(self.dictionaries)
        .with_backends(HashMap::from_iter(self.backends.iter().map(
            |(name, _, port)| {
                (
                    name.clone(),
                    Arc::new(config::Backend {
                        uri: format!("http://localhost:{}", port)
                            .parse::<http::Uri>()
                            .unwrap(),
                        override_host: None,
                        cert_host: None,
                        use_sni: false,
                        grpc: false,
                        client_cert: None,
                        ca_certs: Vec::new(),
                    }),
                )
            },
        )))
        .with_capture_logs(self.captured_logs.clone())
        .with_log_stdout(true)
        .with_log_stderr(true);

        let response = self
            .runtime
            .block_on(ctx.handle_request(
                req,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8000),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8001),
            ))
            .expect("Viceroy response should be successful");

        loop {
            match self.backends.pop() {
                Some((_, stop_server, _)) => {
                    stop_server.send(()).expect("Server should be stopped")
                }
                None => break,
            }
        }

        HandledRequest::new(self.runtime, self.captured_logs, response)
    }
}

pub struct HandledRequest {
    status: u16,
    headers: http::HeaderMap<http::HeaderValue>,
    body_string: String,
    captured_logs: Arc<Mutex<Vec<u8>>>,
}

impl HandledRequest {
    fn new(
        runtime: tokio::runtime::Runtime,
        captured_logs: Arc<Mutex<Vec<u8>>>,
        response: (http::Response<ViceroyBody>, Option<anyhow::Error>),
    ) -> Self {
        Self {
            status: response.0.status().as_u16(),
            headers: response.0.headers().clone(),
            body_string: runtime
                .block_on(response.0.into_body().read_into_string())
                .expect("String body should be in result"),
            captured_logs,
        }
    }

    pub fn get_status(&self) -> u16 {
        self.status
    }

    pub fn get_headers(&self) -> &http::HeaderMap<http::HeaderValue> {
        &self.headers
    }

    pub fn expect_header(&self, name: &str) -> &str {
        self.get_headers()
            .get(name)
            .expect(&format!("{name} header should be returned"))
            .to_str()
            .expect(&format!("{name} header should be a string"))
    }

    pub fn body_string(&self) -> &String {
        &self.body_string
    }

    pub fn captured_logs(&self) -> Vec<(String, String)> {
        Cursor::new(
            self.captured_logs
                .lock()
                .expect("Captured logs should unlock")
                .clone(),
        )
        .lines()
        .map(|l| {
            let line = l.expect("Logs should contain strings");
            let strs = line
                .split_once(" :: ")
                .expect("Logs should be prefixed with the logger endpoint name");

            (String::from(strs.0), String::from(strs.1))
        })
        .collect::<Vec<(String, String)>>()
    }
}
