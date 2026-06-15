// Local behavior proof for the m8 binary Sandbox World (no external site).
//
// Setup: profile must have "sandboxWorld": true. We add a CDP init script that targets the page's
// MAIN world (`page.add_init_script` → addScriptToEvaluateOnNewDocument with no worldName). The Hades
// binary must reroute it into the isolated "util" world. We then check two things on a local page:
//   1. The page's OWN inline (main-world) script cannot see the init script's global
//      → document.title ends in ":undefined"  (DOM is shared across worlds, so the title is readable
//        from either world — it reflects what the MAIN world saw).
//   2. An isolated-world eval DOES see it → window.__hadesProbe === "leaked"
//        (proves the init script actually ran — in the isolated world, not nowhere).
// Both true ⇒ the binary routed the main-world init script into the Sandbox World.
//
//   HADES_LAUNCH_SPEC=C:\tmp\spec.json cargo run --example hades_sandbox_probe
//
// Expected: "SANDBOX WORLD: PASS".

use playwright::Playwright;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchSpec {
    executable_path: String,
    user_data_dir: String,
    #[serde(default)]
    args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    // Respect env so the binary's effect can be isolated. To test the BINARY specifically, run with
    // REBROWSER_PATCHES_RUNTIME_FIX_MODE=0 (stock Playwright: add_init_script targets the MAIN world),
    // then compare the page's main-world view (document.title) with sandboxWorld OFF vs ON.
    if std::env::var_os("REBROWSER_PATCHES_UTILITY_WORLD_NAME").is_none() {
        std::env::set_var("REBROWSER_PATCHES_UTILITY_WORLD_NAME", "util");
    }

    let spec_path = std::env::var("HADES_LAUNCH_SPEC")
        .expect("set HADES_LAUNCH_SPEC to a `hades launch-spec <id> --json` file (sandboxWorld=true)");
    let json = std::fs::read_to_string(&spec_path).expect("read spec");
    let spec: LaunchSpec = serde_json::from_str(json.trim_start_matches('\u{feff}')).expect("parse spec");

    // Write the local probe page: its inline MAIN-world script records whether it can see the global.
    let html = "<!doctype html><html><head><script>\
                document.title = 'probe:' + (typeof window.__hadesProbe);\
                </script></head><body>hades sandbox probe</body></html>";
    let html_path = std::env::temp_dir().join("hades_sandbox_probe.html");
    std::fs::write(&html_path, html).expect("write probe html");
    let url = format!("file:///{}", html_path.display().to_string().replace('\\', "/"));

    let playwright = Playwright::initialize().await?;
    let context = playwright
        .chromium()
        .persistent_context_launcher(&PathBuf::from(&spec.user_data_dir))
        .executable(&PathBuf::from(&spec.executable_path))
        .args(&spec.args)
        .headless(false)
        .launch()
        .await?;
    let page = context.new_page().await?;

    // Init script targets the MAIN world; the binary must reroute it to the isolated world.
    page.add_init_script("window.__hadesProbe = 'leaked';").await?;
    page.goto_builder(&url).goto().await?;

    // document.title is set by the page's OWN main-world inline script — it reflects what the MAIN
    // world saw, regardless of which world we read from (DOM is shared).
    let title: String = page.eval("() => document.title").await?;
    let fix_mode = std::env::var("REBROWSER_PATCHES_RUNTIME_FIX_MODE").unwrap_or_default();

    println!("RUNTIME_FIX_MODE = '{fix_mode}', sandboxWorld profile flag drives the binary");
    println!("document.title (page main-world view): {title}");
    match title.as_str() {
        "probe:undefined" => println!(
            "RESULT: main world CLEAN — the init script did not run in the page's main world."
        ),
        "probe:string" => println!(
            "RESULT: main world LEAKED — the init script ran in the page's main world."
        ),
        other => println!("RESULT: unexpected title '{other}'"),
    }

    context.close().await?;
    Ok(())
}
