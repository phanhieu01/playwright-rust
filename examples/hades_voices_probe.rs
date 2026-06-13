// m7 verify: drive Hades with a pinned profile and read speechSynthesis voices.
// With a macOS profile on a Windows host, getVoices() must return Apple voices
// (Alex/Samantha/...), NOT the host's Microsoft voices.
//
//   set HADES_CONFIG_B64=<base64 of profile.json>   (empty => baseline, host voices)
//   cargo run --example hades_voices_probe
use playwright::Playwright;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let chrome = std::env::var("HADES_CHROME")
        .unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let b64 = std::env::var("HADES_CONFIG_B64").unwrap_or_default();

    let mut args: Vec<String> = Vec::new();
    if !b64.is_empty() {
        args.push(format!("--hades-config={b64}"));
    }

    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();
    let mut launcher = chromium.launcher().executable(Path::new(&chrome)).headless(false);
    if !args.is_empty() {
        launcher = launcher.args(&args);
    }
    let browser = launcher.launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;
    page.goto_builder("about:blank").goto().await?;
    let wait_ms: u64 = std::env::var("HADES_WAIT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(800);
    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

    let voices: serde_json::Value = page
        .eval(
            "() => speechSynthesis.getVoices().map(v => ({name: v.name, lang: v.lang, \
             localService: v.localService, default: v.default}))",
        )
        .await?;
    println!("config = {}", if b64.is_empty() { "NONE (baseline / host)" } else { "Hades profile" });
    println!("{}", serde_json::to_string_pretty(&voices).unwrap());

    browser.close().await?;
    Ok(())
}
