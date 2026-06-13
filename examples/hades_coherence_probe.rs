// Cross-OS coherence gap analysis: drive Hades with a (macOS) profile on this
// Windows host and dump the surfaces that might still leak the host OS, so we can
// rank the remaining m7 work by what actually mismatches a real Mac.
//
//   set HADES_CONFIG_B64=<base64 of macOS profile.json>
//   cargo run --example hades_coherence_probe
use playwright::Playwright;
use std::path::Path;

const PROBE: &str = r#"() => {
  const out = {};
  out.platform = navigator.platform;
  out.uaPlatform = navigator.userAgentData ? navigator.userAgentData.platform : null;
  out.ua = navigator.userAgent;
  out.screen = {
    width: screen.width, height: screen.height,
    availWidth: screen.availWidth, availHeight: screen.availHeight,
    dpr: window.devicePixelRatio,
    innerW: window.innerWidth, innerH: window.innerHeight,
    outerW: window.outerWidth, outerH: window.outerHeight,
    windowFitsScreen: window.outerWidth <= screen.availWidth && window.outerHeight <= screen.availHeight,
  };
  // matchMedia device-width should equal screen.width; if it reveals a DIFFERENT
  // real resolution, screen.* spoof is leaking via CSS media features.
  out.mediaQuery = {
    deviceWidthMatchesScreen: matchMedia('(device-width: ' + screen.width + 'px)').matches,
    dw1920: matchMedia('(device-width: 1920px)').matches,
    dw1280: matchMedia('(device-width: 1280px)').matches,
    widthViewport1280: matchMedia('(width: 1280px)').matches,
    resolutionDpr: matchMedia('(resolution: ' + window.devicePixelRatio + 'dppx)').matches,
  };
  // WebGL: renderer string is spoofed by m3, but the numeric caps / extension list
  // come from the host GPU/driver and may betray Windows/D3D11 under a Mac profile.
  try {
    const gl = document.createElement('canvas').getContext('webgl');
    const dbg = gl.getExtension('WEBGL_debug_renderer_info');
    out.webgl = {
      vendor: gl.getParameter(gl.VENDOR),
      renderer: gl.getParameter(gl.RENDERER),
      unmaskedVendor: dbg ? gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) : null,
      unmaskedRenderer: dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : null,
      maxTextureSize: gl.getParameter(gl.MAX_TEXTURE_SIZE),
      maxViewportDims: Array.from(gl.getParameter(gl.MAX_VIEWPORT_DIMS) || []),
      maxRenderbufferSize: gl.getParameter(gl.MAX_RENDERBUFFER_SIZE),
      aliasedPointSizeRange: Array.from(gl.getParameter(gl.ALIASED_POINT_SIZE_RANGE) || []),
      aliasedLineWidthRange: Array.from(gl.getParameter(gl.ALIASED_LINE_WIDTH_RANGE) || []),
      shadingLanguageVersion: gl.getParameter(gl.SHADING_LANGUAGE_VERSION),
      glVersion: gl.getParameter(gl.VERSION),
      extCount: (gl.getSupportedExtensions() || []).length,
      hasD3DHint: (gl.getSupportedExtensions() || []).join(',').includes('EXT_texture_compression_s3tc'),
    };
  } catch (e) { out.webgl = 'err:' + e; }
  return out;
}"#;

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
    // Secure context so navigator.mediaDevices exists (undefined on about:blank).
    page.goto_builder("https://example.com").goto().await?;
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let webgl: serde_json::Value = page.eval(PROBE).await?;
    // mediaDevices is async; defensive in case the context still lacks it.
    let media: serde_json::Value = page
        .eval(
            "async () => { if (!navigator.mediaDevices) return 'unavailable'; \
             const d = await navigator.mediaDevices.enumerateDevices(); \
             const by = {}; for (const x of d) by[x.kind] = (by[x.kind]||0)+1; \
             return {total: d.length, byKind: by, labels: d.map(x=>x.label)}; }",
        )
        .await?;

    println!("config = {}", if b64.is_empty() { "NONE (host)" } else { "Hades macOS profile" });
    println!("{}", serde_json::to_string_pretty(&webgl).unwrap());
    println!("mediaDevices = {}", serde_json::to_string_pretty(&media).unwrap());

    browser.close().await?;
    Ok(())
}
