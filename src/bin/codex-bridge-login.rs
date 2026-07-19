use codex_bridge::LoginConfig;
use codex_bridge::export_credentials;
use codex_bridge::start_login;
use serde_json::json;
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "Usage:\n  codex-bridge-login login [--codex-home PATH] [--no-open-browser] [--port PORT]\n  codex-bridge-login credentials [--codex-home PATH] [--show-secrets]\n\nLogin uses the ChatGPT browser OAuth flow from the vendored Codex CLI source."
    );
    std::process::exit(2);
}

fn parse_common(args: &[String]) -> (LoginConfig, bool) {
    let mut config = LoginConfig::default();
    let mut show_secrets = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--codex-home" => {
                index += 1;
                let value = args.get(index).unwrap_or_else(|| usage());
                config.codex_home = PathBuf::from(value);
            }
            "--no-open-browser" => config.open_browser = false,
            "--port" => {
                index += 1;
                let value = args.get(index).unwrap_or_else(|| usage());
                config.port = value.parse().unwrap_or_else(|_| usage());
            }
            "--show-secrets" => show_secrets = true,
            _ => usage(),
        }
        index += 1;
    }
    (config, show_secrets)
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        usage();
    }
    let command = args.remove(0);
    let (config, show_secrets) = parse_common(&args);

    match command.as_str() {
        "login" => {
            let session = start_login(config).unwrap_or_else(|error| fatal(error));
            let info = session.start_info();
            eprintln!(
                "ChatGPT login callback server: http://localhost:{}",
                info.callback_port
            );
            eprintln!("Open this URL to sign in:\n\n{}\n", info.auth_url);
            let credentials = session.wait().unwrap_or_else(|error| fatal(error));
            print_credentials(&credentials, show_secrets);
        }
        "credentials" => {
            let credentials = export_credentials(&config).unwrap_or_else(|error| fatal(error));
            print_credentials(&credentials, show_secrets);
        }
        _ => usage(),
    }
}

fn print_credentials(credentials: &codex_bridge::CredentialExport, show_secrets: bool) {
    if show_secrets {
        println!(
            "{}",
            serde_json::to_string_pretty(credentials).expect("serialize credentials")
        );
        return;
    }

    let tokens = credentials.auth.tokens.as_ref();
    let summary = json!({
        "codex_home": credentials.codex_home,
        "auth_mode": credentials.auth.auth_mode,
        "email": tokens.and_then(|tokens| tokens.id_token.email.clone()),
        "account_id": tokens.and_then(|tokens| tokens.account_id.clone())
            .or_else(|| tokens.and_then(|tokens| tokens.id_token.chatgpt_account_id.clone())),
        "plan_type": tokens.and_then(|tokens| tokens.id_token.get_chatgpt_plan_type_raw()),
        "has_access_token": tokens.is_some_and(|tokens| !tokens.access_token.is_empty()),
        "has_refresh_token": tokens.is_some_and(|tokens| !tokens.refresh_token.is_empty()),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).expect("serialize summary")
    );
    eprintln!("Credentials saved. Pass --show-secrets to print the full token payload.");
}

fn fatal(error: impl std::fmt::Display) -> ! {
    eprintln!("error: {error}");
    std::process::exit(1)
}
