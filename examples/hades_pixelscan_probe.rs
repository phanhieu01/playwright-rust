// Drive Hades through pixelscan.net and scrape its verdict. NOTE: with no proxy
// the real host IP won't match the profile's (US) timezone/locale, so geo/IP
// inconsistency is EXPECTED here (a deployment pairs the profile with a matching
// proxy) — focus on the fingerprint-masking / automation checks.
//
//   set HADES_CONFIG_B64=<base64 of profile.json>
//   cargo run --example hades_pixelscan_probe
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
    let url = std::env::var("HADES_URL")
        .unwrap_or_else(|_| "https://pixelscan.net/fingerprint-check".to_string());
    page.goto_builder(&url).goto().await?;
    // Pixelscan runs a battery of async checks; give it generous time to settle.
    let wait: u64 = std::env::var("HADES_WAIT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(30000);
    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;

    let report: serde_json::Value = page
        .eval(
            r#"() => {
              const all = document.body.innerText.split('\n').map(s=>s.trim()).filter(Boolean);
              const grep = (re) => all.filter(l => re.test(l));
              return {
                verdict: grep(/consistent|inconsisten|mask|automation|bot|natural|detect|score|spoof/i).slice(0,25),
                geo: grep(/timezone|time zone|ip\b|location|country|proxy|vpn|geo/i).slice(0,15),
                screen: grep(/resolution|screen|\d{3,4}\s*[x×]\s*\d{3,4}/i).slice(0,10),
                head: all.slice(0, 12),
              };
            }"#,
        )
        .await?;

    println!("config = {}", if b64.is_empty() { "NONE (host)" } else { "Hades profile" });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());

    browser.close().await?;
    Ok(())
}
