// Real-detector test: drive Hades through antcpt's reCAPTCHA v3 score page,
// perform humanize-engine mouse movement + scroll during the observation window,
// then read the displayed score. Compare HADES_HUMAN=1 (default) vs 0 (baseline).
//
// NOTE: reCAPTCHA v3 weighs IP/cookies/fingerprint heavily too, so an un-proxied
// fresh profile scores low regardless — look at the human-vs-baseline DELTA.
//
//   set HADES_HUMAN=0   to disable movement (baseline)
//   set HADES_PRESET=careful
//   cargo run --example hades_recaptcha_v3
use humanize_engine::mouse::{generate_human_move, MouseAction, Point};
use humanize_engine::{scroll, HumanConfig, HumanPreset, HumanRng};
use playwright::api::cdp_session::CdpSession;
use playwright::Playwright;
use serde_json::json;
use std::path::Path;

const URL: &str = "https://antcpt.com/eng/information/demo-form/recaptcha-3-test-score.html";

async fn mev(cdp: &CdpSession, ty: &str, x: f64, y: f64, button: &str, buttons: i64, dx: f64, dy: f64) {
    let p = json!({"type": ty, "x": x, "y": y, "button": button, "buttons": buttons,
        "clickCount": if ty=="mousePressed"||ty=="mouseReleased" {1} else {0}, "deltaX": dx, "deltaY": dy});
    let _ = cdp.send("Input.dispatchMouseEvent", p.as_object().cloned()).await;
}
async fn nap(ms: f64) { if ms > 0.0 { tokio::time::sleep(std::time::Duration::from_micros((ms*1000.0) as u64)).await; } }
async fn exec(cdp: &CdpSession, actions: &[MouseAction], pos: &mut (f64, f64)) {
    for a in actions {
        match a {
            MouseAction::Move { x, y, delay_ms } => { *pos = (*x,*y); mev(cdp,"mouseMoved",*x,*y,"none",0,0.0,0.0).await; nap(*delay_ms).await; }
            MouseAction::Down => mev(cdp,"mousePressed",pos.0,pos.1,"left",1,0.0,0.0).await,
            MouseAction::Up => mev(cdp,"mouseReleased",pos.0,pos.1,"left",0,0.0,0.0).await,
            MouseAction::Wheel { delta_x, delta_y } => mev(cdp,"mouseWheel",pos.0,pos.1,"none",0,*delta_x,*delta_y).await,
            MouseAction::Sleep { ms } => nap(*ms).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let chrome = std::env::var("HADES_CHROME").unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let human = std::env::var("HADES_HUMAN").as_deref() != Ok("0");
    let cfg = match std::env::var("HADES_PRESET").as_deref() { Ok("careful") => HumanConfig::careful(), _ => HumanConfig::default() };
    let _ = HumanPreset::Default;

    let mut args = vec!["--disable-blink-features=AutomationControlled".to_string()];
    if let Ok(b64) = std::env::var("HADES_CONFIG_B64") { if !b64.is_empty() { args.push(format!("--hades-config={b64}")); } }

    let pw = Playwright::initialize().await?;
    let browser = pw.chromium().launcher().executable(Path::new(&chrome)).headless(false).args(&args).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;
    page.goto_builder(URL).goto().await?;
    tokio::time::sleep(std::time::Duration::from_secs(7)).await; // grecaptcha init

    let cdp = context.new_cdp_session(&page).await?;
    if human {
        let mut rng = HumanRng::new(7);
        let mut pos = (150.0, 200.0);
        // Wander between several viewport points + scroll, ~12s of human behavior.
        let waypoints = [(850.0,300.0),(400.0,550.0),(1000.0,420.0),(300.0,250.0),(700.0,600.0)];
        for (wx, wy) in waypoints {
            let mv = generate_human_move(&mut rng, Point::new(pos.0, pos.1), Point::new(wx, wy), &cfg);
            exec(&cdp, &mv, &mut pos).await;
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        }
        let sc = scroll::generate_smooth_wheel(&mut rng, 800);
        exec(&cdp, &sc, &mut pos).await;
    }

    // Trigger a fresh score check (click "Refresh score now!") and read the result.
    let _: serde_json::Value = page.eval(
        r#"() => { const el=[...document.querySelectorAll('button,a,input')].find(e=>/refresh score/i.test((e.textContent||e.value||''))); if(el) el.click(); return null; }"#,
    ).await.unwrap_or(serde_json::Value::Null);
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;

    let score: serde_json::Value = page.eval(
        r#"() => { const txt=document.body.innerText; const m=txt.match(/0\.\d+/g)||[]; const line=(txt.split('\n').find(l=>/score/i.test(l)&&/0\.\d/.test(l))||'').trim(); return {scoreNumbers:m.slice(0,5), scoreLine:line}; }"#,
    ).await?;

    println!("human movement = {human}");
    println!("{}", serde_json::to_string_pretty(&score).unwrap());
    browser.close().await?;
    Ok(())
}
