use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../upstream/codex/.git");
    let commit = Command::new("git")
        .args(["-C", "../upstream/codex", "rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=CODEX_HTTP_BRIDGE_CODEX_COMMIT={commit}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libcodex_http_bridge.dylib");
    }
}
