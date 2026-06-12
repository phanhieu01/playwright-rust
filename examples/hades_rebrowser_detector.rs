// Drive the live rebrowser bot-detector with Hades + (patched) playwright-rust and
// scrape its verdicts. The detector's `mainWorldExecution` / `runtimeEnableLeak`
// tests are the ones the rebrowser driver patch is meant to flip green.
//
//   cargo run --example hades_rebrowser_detector
use playwright::Playwright;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let chrome = std::env::var("HADES_CHROME")
        .unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let url = std::env::var("HADES_URL")
        .unwrap_or_else(|_| "https://bot-detector.rebrowser.net/".to_string());

    // Extra chrome args, comma-separated, e.g.
    //   HADES_ARGS=--disable-blink-features=AutomationControlled
    let extra: Vec<String> = std::env::var("HADES_ARGS")
        .ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();

    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();
    let mut launcher = chromium
        .launcher()
        .executable(Path::new(&chrome))
        .headless(false);
    if !extra.is_empty() {
        launcher = launcher.args(&extra);
    }
    let browser = launcher.launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;
    page.goto_builder(&url).goto().await?;

    // Let the in-page detector battery finish, then trigger a main-world evaluate
    // (this is exactly what a real automation does and what mainWorldExecution traps).
    tokio::time::sleep(std::time::Duration::from_secs(7)).await;
    let _: serde_json::Value = page.eval("() => document.title").await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let text: String = page
        .eval("() => document.body ? document.body.innerText : '<no body>'")
        .await?;
    println!("URL = {url}");
    println!("================ PAGE TEXT ================");
    println!("{text}");
    println!("==========================================");

    browser.close().await?;
    Ok(())
}
