// Smoke + CDP-residue test: drive the Hades browser with playwright-rust and run
// the detector battery. With the rebrowser-patched driver embedded, the
// runtime-enable leak probes (console getter firing) must report false.
//
//   set HADES_CHROME=C:\hb\ucw\build\src\out\Default\chrome.exe   (optional override)
//   cargo run --example hades_stealth_smoke
use playwright::Playwright;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let chrome = std::env::var("HADES_CHROME")
        .unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let botdetect = std::fs::read_to_string(r"C:\hb\e2e\botdetect.js").expect("botdetect.js");
    let cdpdetect = std::fs::read_to_string(r"C:\hb\e2e\cdpdetect.js").expect("cdpdetect.js");

    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();
    let browser = chromium
        .launcher()
        .executable(Path::new(&chrome))
        .headless(false)
        .launch()
        .await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;
    page.goto_builder("about:blank").goto().await?;

    // Each *.js file is an invoked IIFE `(() => {...})()`; wrap it in a function
    // literal so playwright evaluates it and returns the result object.
    let bot: serde_json::Value = page
        .eval(&format!("() => {{ return ({botdetect}); }}"))
        .await?;
    let cdp: serde_json::Value = page
        .eval(&format!("() => {{ return ({cdpdetect}); }}"))
        .await?;

    println!("HADES_CHROME = {chrome}");
    println!("--- botdetect.js ---");
    println!("{}", serde_json::to_string_pretty(&bot).unwrap());
    println!("--- cdpdetect.js (runtime-enable leak) ---");
    println!("{}", serde_json::to_string_pretty(&cdp).unwrap());

    // Hard asserts on the residue signals the rebrowser patch must fix.
    let webdriver = bot.get("webdriver").and_then(|v| v.as_bool());
    let cdp_globals = bot.get("cdpGlobals").and_then(|v| v.as_array());
    let getter = cdp.get("consoleGetterFired").and_then(|v| v.as_bool());
    let err_stack = cdp.get("errorStackGetterFired").and_then(|v| v.as_bool());
    println!("\n=== VERDICT ===");
    println!("webdriver == false        : {:?}", webdriver == Some(false));
    println!("cdpGlobals empty          : {:?}", cdp_globals.map(|a| a.is_empty()));
    println!("consoleGetterFired false  : {:?}", getter == Some(false));
    println!("errorStackGetterFired false: {:?}", err_stack == Some(false));

    browser.close().await?;
    Ok(())
}
