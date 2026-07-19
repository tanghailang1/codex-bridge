use crate::HttpBridgeConfig;
use crate::HttpBridgeError;
use crate::HttpRequest;
use crate::event::event_to_json;
use codex_api::ApiError;
use codex_api::AuthProvider;
use codex_api::Compression;
use codex_api::Provider;
use codex_api::ResponsesClient;
use codex_api::TransportError;
use codex_api::build_session_headers;
use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_http_client::ReqwestTransport;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_login::default_client::build_default_reqwest_client_for_route;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::ThreadId;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderName;
use http::HeaderValue;
use http::StatusCode;
use serde_json::Value;
use serde_json::json;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::watch;
use uuid::Uuid;

const INSTALLATION_ID_HEADER: &str = "x-codex-installation-id";
const WINDOW_ID_HEADER: &str = "x-codex-window-id";

struct ManagedChatGptAuth {
    manager: Arc<AuthManager>,
    expected_account_id: Option<String>,
    expected_user_id: Option<String>,
}

impl AuthProvider for ManagedChatGptAuth {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        let Some(auth) = self
            .manager
            .auth_cached()
            .filter(CodexAuth::is_chatgpt_auth)
        else {
            return;
        };
        if auth.get_account_id() != self.expected_account_id
            || auth.get_chatgpt_user_id() != self.expected_user_id
        {
            return;
        }
        if let Ok(token) = auth.get_token()
            && let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}"))
        {
            headers.insert(http::header::AUTHORIZATION, value);
        }
        if let Some(account_id) = auth.get_account_id()
            && let Ok(value) = HeaderValue::from_str(&account_id)
        {
            headers.insert("ChatGPT-Account-ID", value);
        }
        if auth.is_fedramp_account() {
            headers.insert("X-OpenAI-Fedramp", HeaderValue::from_static("true"));
        }
    }
}

struct HttpEngine {
    config: HttpBridgeConfig,
    auth_manager: Arc<AuthManager>,
    client: Arc<ResponsesClient<ReqwestTransport>>,
    installation_id: String,
}

pub struct HttpBridge {
    runtime: Runtime,
    engine: Arc<HttpEngine>,
}

pub enum NextEvent {
    Event(String),
    Timeout,
    Closed,
}

pub struct HttpRequestHandle {
    receiver: Mutex<mpsc::Receiver<String>>,
    cancel: watch::Sender<bool>,
}

impl HttpRequestHandle {
    pub fn next_event(&self, timeout: Option<Duration>) -> Result<NextEvent, HttpBridgeError> {
        let receiver = self
            .receiver
            .try_lock()
            .map_err(|_| HttpBridgeError::ReceiverBusy)?;
        let result = match timeout {
            Some(timeout) => receiver.recv_timeout(timeout),
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match result {
            Ok(event) => Ok(NextEvent::Event(event)),
            Err(RecvTimeoutError::Timeout) => Ok(NextEvent::Timeout),
            Err(RecvTimeoutError::Disconnected) => Ok(NextEvent::Closed),
        }
    }

    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }
}

impl HttpBridge {
    pub fn new(config: HttpBridgeConfig) -> Result<Self, HttpBridgeError> {
        let runtime = Runtime::new().map_err(HttpBridgeError::Runtime)?;
        let engine = runtime.block_on(HttpEngine::new(config))?;
        Ok(Self {
            runtime,
            engine: Arc::new(engine),
        })
    }

    pub fn start(&self, request: HttpRequest) -> HttpRequestHandle {
        let (event_tx, event_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let engine = Arc::clone(&self.engine);
        self.runtime.spawn(async move {
            engine.run(request, event_tx, cancel_rx).await;
        });
        HttpRequestHandle {
            receiver: Mutex::new(event_rx),
            cancel: cancel_tx,
        }
    }
}

impl HttpEngine {
    async fn new(config: HttpBridgeConfig) -> Result<Self, HttpBridgeError> {
        let auth_manager = Arc::new(
            AuthManager::new(
                config.codex_home.clone(),
                /*enable_codex_api_key_env*/ false,
                config.credentials_store_mode,
                /*forced_chatgpt_workspace_id*/ None,
                /*chatgpt_base_url*/ None,
                config.keyring_backend_kind,
                /*auth_route_config*/ None,
            )
            .await,
        );
        let auth = auth_manager
            .auth()
            .await
            .ok_or(HttpBridgeError::NoCredentials)?;
        if !auth.is_chatgpt_auth() {
            return Err(HttpBridgeError::NotChatGpt);
        }

        let mut provider_info = ModelProviderInfo::create_openai_provider(config.base_url.clone());
        provider_info.requires_openai_auth = true;
        provider_info.supports_websockets = false;
        let provider: Provider = provider_info
            .to_api_provider(Some(auth.auth_mode()))
            .map_err(|error| HttpBridgeError::Provider(error.to_string()))?;

        let request_url = provider.url_for_path("responses");
        let factory = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault);
        let reqwest_client =
            build_default_reqwest_client_for_route(&factory, &request_url, ClientRouteClass::Api)
                .map_err(|error| HttpBridgeError::HttpClient(error.to_string()))?;
        let transport = ReqwestTransport::new(reqwest_client);
        let auth_provider: Arc<dyn AuthProvider> = Arc::new(ManagedChatGptAuth {
            manager: Arc::clone(&auth_manager),
            expected_account_id: auth.get_account_id(),
            expected_user_id: auth.get_chatgpt_user_id(),
        });
        let client = Arc::new(ResponsesClient::new(transport, provider, auth_provider));
        let installation_id =
            resolve_installation_id(&config.codex_home).map_err(HttpBridgeError::Runtime)?;

        Ok(Self {
            config,
            auth_manager,
            client,
            installation_id,
        })
    }

    async fn run(
        &self,
        mut request: HttpRequest,
        events: mpsc::Sender<String>,
        mut cancel: watch::Receiver<bool>,
    ) {
        if let Err(error) = self.run_inner(&mut request, &events, &mut cancel).await {
            send_json(
                &events,
                json!({"type":"bridge.error","message":error.to_string()}),
            );
        }
        send_json(&events, json!({"type":"bridge.closed"}));
    }

    async fn run_inner(
        &self,
        request: &mut HttpRequest,
        events: &mpsc::Sender<String>,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<(), HttpBridgeError> {
        let thread_id = request
            .thread_id
            .clone()
            .unwrap_or_else(|| ThreadId::new().to_string());
        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| thread_id.clone());
        let installation_id = request
            .installation_id
            .clone()
            .unwrap_or_else(|| self.installation_id.clone());
        let window_id = request
            .window_id
            .clone()
            .unwrap_or_else(|| format!("{thread_id}:0"));
        request.normalize_body(&session_id, &thread_id, &installation_id, &window_id)?;

        let headers = self.request_headers(
            request,
            &session_id,
            &thread_id,
            &installation_id,
            &window_id,
        )?;
        let compression = if self.config.request_compression {
            Compression::Zstd
        } else {
            Compression::None
        };
        let turn_state = Arc::new(OnceLock::new());

        let mut stream = match self
            .client
            .stream(
                request.body.clone(),
                headers.clone(),
                compression,
                Some(Arc::clone(&turn_state)),
            )
            .await
        {
            Ok(stream) => stream,
            Err(error) if is_unauthorized(&error) => {
                self.auth_manager
                    .refresh_token()
                    .await
                    .map_err(|refresh| HttpBridgeError::Request(refresh.to_string()))?;
                self.client
                    .stream(request.body.clone(), headers, compression, Some(turn_state))
                    .await
                    .map_err(|error| HttpBridgeError::Request(error.to_string()))?
            }
            Err(error) => return Err(HttpBridgeError::Request(error.to_string())),
        };

        send_json(
            events,
            json!({
                "type":"bridge.upstream_connected",
                "upstream_request_id":stream.upstream_request_id,
                "session_id":session_id,
                "thread_id":thread_id,
            }),
        );

        loop {
            if *cancel.borrow() {
                send_json(events, json!({"type":"bridge.cancelled"}));
                return Ok(());
            }
            tokio::select! {
                changed = cancel.changed() => {
                    if changed.is_ok() && *cancel.borrow() {
                        send_json(events, json!({"type":"bridge.cancelled"}));
                        return Ok(());
                    }
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(event)) => send_json(events, event_to_json(event)),
                        Some(Err(error)) => return Err(HttpBridgeError::Request(error.to_string())),
                        None => return Ok(()),
                    }
                }
            }
        }
    }

    fn request_headers(
        &self,
        request: &HttpRequest,
        session_id: &str,
        thread_id: &str,
        installation_id: &str,
        window_id: &str,
    ) -> Result<HeaderMap, HttpBridgeError> {
        let mut headers =
            build_session_headers(Some(session_id.to_string()), Some(thread_id.to_string()));
        insert_header(&mut headers, "x-client-request-id", thread_id)?;
        insert_header(&mut headers, INSTALLATION_ID_HEADER, installation_id)?;
        insert_header(&mut headers, WINDOW_ID_HEADER, window_id)?;
        if let Some(beta) = self
            .config
            .beta_features
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            insert_header(&mut headers, "x-codex-beta-features", beta)?;
        }
        for (name, value) in &request.extra_headers {
            let name = HeaderName::try_from(name).map_err(|error| {
                HttpBridgeError::InvalidRequest(format!("invalid header name: {error}"))
            })?;
            let value = HeaderValue::try_from(value).map_err(|error| {
                HttpBridgeError::InvalidRequest(format!("invalid header value: {error}"))
            })?;
            headers.insert(name, value);
        }
        Ok(headers)
    }
}

fn insert_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), HttpBridgeError> {
    let value = HeaderValue::from_str(value).map_err(|error| {
        HttpBridgeError::InvalidRequest(format!("invalid {name} header: {error}"))
    })?;
    headers.insert(name, value);
    Ok(())
}

fn is_unauthorized(error: &ApiError) -> bool {
    matches!(
        error,
        ApiError::Transport(TransportError::Http { status, .. }) if *status == StatusCode::UNAUTHORIZED
    )
}

fn send_json(sender: &mpsc::Sender<String>, value: Value) {
    let _ = sender.send(value.to_string());
}

fn resolve_installation_id(codex_home: &Path) -> std::io::Result<String> {
    std::fs::create_dir_all(codex_home)?;
    let path = codex_home.join("installation_id");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let mut file = options.open(path)?;
    let mut existing = String::new();
    file.read_to_string(&mut existing)?;
    if let Ok(id) = Uuid::parse_str(existing.trim()) {
        return Ok(id.to_string());
    }
    let id = Uuid::new_v4().to_string();
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(id.as_bytes())?;
    file.flush()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpRequest;
    use serde_json::json;
    use std::io::Read;
    use std::io::Write;
    use std::net::TcpListener;

    const TEST_ID_TOKEN: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJlbWFpbCI6InVzZXJAZXhhbXBsZS5jb20iLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF91c2VyX2lkIjoidXNlci0xMjM0NSIsImNoYXRncHRfcGxhbl90eXBlIjoicHJvIiwiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjb3VudC0xMjMifX0.c2ln";

    #[test]
    fn sends_through_codex_transport_and_parses_sse() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept request");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = socket.read(&mut buffer).expect("read request");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                let Some(header_end) = find_bytes(&request, b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
            let request_lower = request_text.to_ascii_lowercase();
            assert!(request_lower.contains("authorization: bearer test-access"));
            assert!(request_lower.contains("chatgpt-account-id: account-123"));
            assert!(request_lower.contains("session-id: test-session"));
            assert!(request_lower.contains("thread-id: test-thread"));

            let body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\"}}\n\n"
            );
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nx-request-id: req-test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let home = tempfile::tempdir().expect("temp home");
        std::fs::write(
            home.path().join("auth.json"),
            serde_json::to_vec(&json!({
                "auth_mode":"chatgpt",
                "OPENAI_API_KEY":null,
                "tokens":{
                    "id_token":TEST_ID_TOKEN,
                    "access_token":"test-access",
                    "refresh_token":"test-refresh",
                    "account_id":"account-123"
                },
                "last_refresh":"2099-01-01T00:00:00Z"
            }))
            .expect("serialize auth"),
        )
        .expect("write auth");

        let bridge = HttpBridge::new(HttpBridgeConfig {
            codex_home: home.path().to_path_buf(),
            base_url: Some(format!("http://{address}/v1")),
            request_compression: false,
            ..HttpBridgeConfig::default()
        })
        .expect("create bridge");
        let handle = bridge.start(HttpRequest {
            body: json!({
                "model":"gpt-test",
                "input":[{
                    "type":"message",
                    "role":"user",
                    "content":[{"type":"input_text","text":"hello"}]
                }]
            }),
            session_id: Some("test-session".to_string()),
            thread_id: Some("test-thread".to_string()),
            ..HttpRequest::default()
        });

        let mut events = Vec::new();
        loop {
            match handle
                .next_event(Some(Duration::from_secs(5)))
                .expect("next event")
            {
                NextEvent::Event(event) => events.push(event),
                NextEvent::Timeout => panic!("timed out waiting for bridge event"),
                NextEvent::Closed => break,
            }
        }
        server.join().expect("server thread");
        assert!(
            events
                .iter()
                .any(|event| event.contains("upstream_connected"))
        );
        assert!(
            events
                .iter()
                .any(|event| event.contains("output_text.delta"))
        );
        assert!(
            events
                .iter()
                .any(|event| event.contains("response.completed"))
        );
        assert!(!events.iter().any(|event| event.contains("bridge.error")));
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}
