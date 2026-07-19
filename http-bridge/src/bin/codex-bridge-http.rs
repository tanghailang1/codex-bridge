use codex_http_bridge::HttpBridge;
use codex_http_bridge::HttpBridgeConfig;
use codex_http_bridge::HttpRequest;
use codex_http_bridge::NextEvent;
use serde_json::Value;
use serde_json::json;
use std::io::Read;
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "Usage:\n  codex-bridge-http request --body FILE|- [--codex-home PATH] [--base-url URL] [--no-normalize] [--no-compression]\n\nThe body file is an OpenAI Responses request JSON object. Events are printed as NDJSON."
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("request") {
        usage();
    }

    let mut config = HttpBridgeConfig::default();
    let mut body_path = None;
    let mut normalize = true;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--body" => body_path = args.next(),
            "--codex-home" => {
                config.codex_home = PathBuf::from(args.next().unwrap_or_else(|| usage()))
            }
            "--base-url" => config.base_url = Some(args.next().unwrap_or_else(|| usage())),
            "--no-normalize" => normalize = false,
            "--no-compression" => config.request_compression = false,
            _ => usage(),
        }
    }
    let body_path = body_path.unwrap_or_else(|| usage());
    let body = read_body(&body_path).unwrap_or_else(|error| fatal(error));
    let body: Value = serde_json::from_slice(&body).unwrap_or_else(|error| fatal(error));
    let request = HttpRequest::from_json(
        &serde_json::to_vec(&json!({"body":body,"normalize":normalize}))
            .expect("serialize request envelope"),
    )
    .unwrap_or_else(|error| fatal(error));

    let bridge = HttpBridge::new(config).unwrap_or_else(|error| fatal(error));
    let handle = bridge.start(request);
    loop {
        match handle.next_event(None).unwrap_or_else(|error| fatal(error)) {
            NextEvent::Event(event) => println!("{event}"),
            NextEvent::Timeout => continue,
            NextEvent::Closed => break,
        }
    }
}

fn read_body(path: &str) -> std::io::Result<Vec<u8>> {
    if path == "-" {
        let mut body = Vec::new();
        std::io::stdin().read_to_end(&mut body)?;
        Ok(body)
    } else {
        std::fs::read(path)
    }
}

fn fatal(error: impl std::fmt::Display) -> ! {
    eprintln!("error: {error}");
    std::process::exit(1)
}
