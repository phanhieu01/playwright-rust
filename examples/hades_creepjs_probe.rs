// Comprehensive validation: drive Hades with a (macOS) profile through CreepJS and
// scrape its verdict (trust score + detected lies), to surface any cross-OS
// inconsistency the targeted probes missed.
//
//   set HADES_CONFIG_B64=<base64 of macOS profile.json>
//   cargo run --example hades_creepjs_probe
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
    page.goto_builder("https://abrahamjuliot.github.io/creepjs/").goto().await?;
    // CreepJS computes asynchronously; give it time to settle.
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    // Pull the headline verdict + anything that looks like a lie / OS mismatch.
    let report: serde_json::Value = page
        .eval(
            r#"() => {
              const txt = (sel) => { const e = document.querySelector(sel); return e ? e.innerText.trim() : null; };
              const all = document.body.innerText.split('\n').map(s=>s.trim()).filter(Boolean);
              const grep = (re) => all.filter(l => re.test(l));
              // Context around any line mentioning a lie, to capture the offending property.
              const ctx = [];
              all.forEach((l,i) => { if (/\blie(s)?\b|lied|forced|\bprototype\b/i.test(l)) ctx.push(all.slice(i, i+4).join(' | ')); });
              return {
                verdict: grep(/trust|lies|\bgrade\b|score|bot|%/i).slice(0,25),
                fontHints: grep(/segoe|san francisco|helvetica|apple-system|platform hints/i).slice(0,10),
                osLeakWindows: grep(/windows|win32|direct3d|d3d11|microsoft/i).slice(0,15),
              };
            }"#,
        )
        .await?;

    println!("config = {}", if b64.is_empty() { "NONE (host)" } else { "Hades macOS profile" });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());

    browser.close().await?;
    Ok(())
}
