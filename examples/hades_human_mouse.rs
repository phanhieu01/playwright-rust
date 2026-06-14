// Execute humanize-engine mouse paths as REAL input on Hades via CDP
// (Input.dispatchMouseEvent), then read back the page-recorded trail to confirm
// the movement arrives as genuine events with a human shape/timing.
//
//   set HADES_PRESET=careful   (default|careful)
//   set HADES_CONFIG_B64=...    (optional profile)
//   cargo run --example hades_human_mouse
use humanize_engine::mouse::{click_target, generate_click, generate_human_move, BoundingBox, MouseAction, Point};
use humanize_engine::{HumanConfig, HumanPreset, HumanRng};
use playwright::api::cdp_session::CdpSession;
use playwright::Playwright;
use serde_json::json;
use std::path::Path;

async fn mouse_event(cdp: &CdpSession, ty: &str, x: f64, y: f64, button: &str, buttons: i64, dx: f64, dy: f64) {
    let p = json!({
        "type": ty, "x": x, "y": y, "button": button, "buttons": buttons,
        "clickCount": if ty == "mousePressed" || ty == "mouseReleased" { 1 } else { 0 },
        "deltaX": dx, "deltaY": dy,
    });
    let _ = cdp.send("Input.dispatchMouseEvent", p.as_object().cloned()).await;
}

async fn sleep_ms(ms: f64) {
    if ms > 0.0 {
        tokio::time::sleep(std::time::Duration::from_micros((ms * 1000.0) as u64)).await;
    }
}

/// Execute a mouse action list, tracking the cursor position.
async fn exec(cdp: &CdpSession, actions: &[MouseAction], pos: &mut (f64, f64)) {
    for a in actions {
        match a {
            MouseAction::Move { x, y, delay_ms } => {
                *pos = (*x, *y);
                mouse_event(cdp, "mouseMoved", *x, *y, "none", 0, 0.0, 0.0).await;
                sleep_ms(*delay_ms).await;
            }
            MouseAction::Down => mouse_event(cdp, "mousePressed", pos.0, pos.1, "left", 1, 0.0, 0.0).await,
            MouseAction::Up => mouse_event(cdp, "mouseReleased", pos.0, pos.1, "left", 0, 0.0, 0.0).await,
            MouseAction::Wheel { delta_x, delta_y } => {
                mouse_event(cdp, "mouseWheel", pos.0, pos.1, "none", 0, *delta_x, *delta_y).await;
            }
            MouseAction::Sleep { ms } => sleep_ms(*ms).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let chrome = std::env::var("HADES_CHROME")
        .unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let preset = match std::env::var("HADES_PRESET").as_deref() {
        Ok("careful") => HumanPreset::Careful,
        _ => HumanPreset::Default,
    };
    let cfg = match preset {
        HumanPreset::Careful => HumanConfig::careful(),
        HumanPreset::Default => HumanConfig::default(),
    };

    let mut args = vec!["--disable-blink-features=AutomationControlled".to_string()];
    if let Ok(b64) = std::env::var("HADES_CONFIG_B64") {
        if !b64.is_empty() {
            args.push(format!("--hades-config={b64}"));
        }
    }

    let pw = Playwright::initialize().await?;
    let chromium = pw.chromium();
    let browser = chromium.launcher().executable(Path::new(&chrome)).headless(false).args(&args).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;
    page.goto_builder("about:blank").goto().await?;

    // Inject a button at a known viewport position + a mousemove/click recorder.
    page.eval::<()>(
        r#"() => {
          document.body.style.margin = '0';
          const b = document.createElement('button');
          b.id = 'b'; b.textContent = 'Click';
          b.style.cssText = 'position:absolute;left:700px;top:450px;width:120px;height:40px';
          document.body.appendChild(b);
          window.__trail = []; window.__clicked = 0;
          addEventListener('mousemove', e => window.__trail.push([e.clientX, e.clientY, performance.now()]), true);
          b.addEventListener('click', () => window.__clicked++);
        }"#,
    ).await?;

    let cdp = context.new_cdp_session(&page).await?;
    let mut rng = HumanRng::new(42);
    let mut pos = (100.0, 120.0);
    // Off-center click target inside the button's box (NOT dead center) via click_target.
    let bbox = BoundingBox { x: 700.0, y: 450.0, width: 120.0, height: 40.0 };
    let target = click_target(&mut rng, &bbox, false, &cfg);
    let center = (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0);
    println!("button box center=({:.0},{:.0}); click_target=({:.0},{:.0}) offset=({:+.0},{:+.0})px",
        center.0, center.1, target.x, target.y, target.x - center.0, target.y - center.1);
    let mv = generate_human_move(&mut rng, Point::new(pos.0, pos.1), target, &cfg);
    let t0 = std::time::Instant::now();
    exec(&cdp, &mv, &mut pos).await;
    let click = generate_click(&mut rng, false, &cfg);
    exec(&cdp, &click, &mut pos).await;
    let elapsed = t0.elapsed().as_millis();

    // Read back the recorded trail and compute human-ness metrics.
    let report: serde_json::Value = page.eval(
        r#"() => {
          const t = window.__trail;
          if (t.length < 2) return {points: t.length, clicked: window.__clicked};
          let path = 0;
          for (let i = 1; i < t.length; i++) path += Math.hypot(t[i][0]-t[i-1][0], t[i][1]-t[i-1][1]);
          const eucl = Math.hypot(t[t.length-1][0]-t[0][0], t[t.length-1][1]-t[0][1]);
          const dur = t[t.length-1][2] - t[0][2];
          return {
            points: t.length, clicked: window.__clicked,
            durationMs: Math.round(dur),
            straightness: +(path / eucl).toFixed(3),
            start: [t[0][0], t[0][1]], end: [t[t.length-1][0], t[t.length-1][1]],
          };
        }"#,
    ).await?;

    println!("preset = {preset:?}, wall elapsed = {elapsed}ms");
    println!("{}", serde_json::to_string_pretty(&report).unwrap());

    browser.close().await?;
    Ok(())
}
