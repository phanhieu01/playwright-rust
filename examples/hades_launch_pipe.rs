// Launch a Hades *profile* through playwright-rust over the **pipe** transport (Camoufox-style),
// with the automation agent pinned to an isolated "Sandbox World" so the page's main world never
// sees automation.
//
// Why this exists (vs. the other examples):
//   * The other hades_* examples use `chromium.launcher()` with an *ephemeral* profile and no Hades
//     config — fine for surface probes, but not a real profile run.
//   * This one consumes the launcher contract `hades launch-spec <id> --json` (executablePath +
//     userDataDir + args, incl. --hades-config-file / --proxy-server) and launches the *persistent*
//     context, so cookies + the portable-crypto profile + the spoof config are all active.
//   * `launchPersistentContext` (like every Playwright Chromium launch) uses `--remote-debugging-pipe`
//     by default: NO listening TCP port, NO /json HTTP endpoints. That is the "luồng khác" Camoufox
//     uses — the control channel rides an inherited pipe in the privileged process, invisible to the
//     page and to local port scanners.
//   * `REBROWSER_PATCHES_RUNTIME_FIX_MODE=alwaysIsolated` forces every evaluate into the utility
//     (isolated) world — the Chromium equivalent of Camoufox's Sandbox World: automation bindings and
//     evaluations live in a world the page's main DOM/JS cannot observe.
//
// Usage:
//   # 1. in the hadesbrowser repo, materialize the contract for a profile:
//   #    hades launch-spec <id> --json > C:\tmp\spec.json
//   # 2. point this example at it (+ a target page to inspect):
//   HADES_LAUNCH_SPEC=C:\tmp\spec.json \
//   HADES_URL=https://bot-detector.rebrowser.net/ \
//   cargo run --example hades_launch_pipe
//
// Falls back to HADES_CHROME + HADES_USER_DATA_DIR (+ optional HADES_CONFIG) if no spec file is set.

use playwright::Playwright;
use serde::Deserialize;
use std::path::PathBuf;

/// The JSON emitted by `hades launch-spec <id> --json` (camelCase keys).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchSpec {
    executable_path: String,
    user_data_dir: String,
    #[serde(default)]
    args: Vec<String>,
}

fn load_spec() -> LaunchSpec {
    if let Ok(path) = std::env::var("HADES_LAUNCH_SPEC") {
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read HADES_LAUNCH_SPEC ({path}): {e}"));
        // Tolerate a UTF-8 BOM (PowerShell's `Out-File -Encoding utf8` writes one).
        let json = json.trim_start_matches('\u{feff}');
        return serde_json::from_str(json)
            .unwrap_or_else(|e| panic!("bad launch-spec JSON ({path}): {e}"));
    }
    // Fallback: build a minimal spec from individual env vars.
    let executable_path = std::env::var("HADES_CHROME")
        .unwrap_or_else(|_| r"C:\hb\ucw\build\src\out\Default\chrome.exe".to_string());
    let user_data_dir = std::env::var("HADES_USER_DATA_DIR")
        .expect("set HADES_LAUNCH_SPEC, or HADES_USER_DATA_DIR for the fallback");
    let mut args = Vec::new();
    if let Ok(cfg) = std::env::var("HADES_CONFIG") {
        args.push(format!("--hades-config-file={cfg}"));
    }
    LaunchSpec { executable_path, user_data_dir, args }
}

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    // Pin the agent to the isolated Sandbox World. Must be set BEFORE the Node driver is spawned by
    // `Playwright::initialize()` (the driver inherits this process's env). Respect an explicit
    // override if the caller already set it.
    if std::env::var_os("REBROWSER_PATCHES_RUNTIME_FIX_MODE").is_none() {
        std::env::set_var("REBROWSER_PATCHES_RUNTIME_FIX_MODE", "alwaysIsolated");
    }
    // Pin the driver's utility-world name to "util" so it coincides with the world the Hades binary
    // routes main-world init scripts into (m8 sandbox-world, profile field "sandboxWorld": true).
    // Same world ⇒ init-script state and evaluate() see each other.
    if std::env::var_os("REBROWSER_PATCHES_UTILITY_WORLD_NAME").is_none() {
        std::env::set_var("REBROWSER_PATCHES_UTILITY_WORLD_NAME", "util");
    }

    let spec = load_spec();
    let url = std::env::var("HADES_URL")
        .unwrap_or_else(|_| "https://bot-detector.rebrowser.net/".to_string());

    let exe = PathBuf::from(&spec.executable_path);
    let udd = PathBuf::from(&spec.user_data_dir);
    println!("launching (pipe, no port): {}", exe.display());
    println!("  user-data-dir: {}", udd.display());
    println!("  args         : {}", spec.args.join(" "));
    println!("  agent world  : isolated (REBROWSER_PATCHES_RUNTIME_FIX_MODE=alwaysIsolated)");

    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();

    // Persistent context over the default pipe transport. We pass our own --user-data-dir-free args;
    // Playwright supplies the user-data-dir and the pipe itself.
    let context = chromium
        .persistent_context_launcher(&udd)
        .executable(&exe)
        .args(&spec.args)
        .headless(false)
        .launch()
        .await?;

    let page = context.new_page().await?;
    page.goto_builder(&url).goto().await?;

    // Give any in-page detector battery time to run, then do a main-world evaluate — exactly what a
    // real automation does, and what `mainWorldExecution` traps. With alwaysIsolated this must NOT
    // surface a main-world execution-context leak.
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

    context.close().await?;
    Ok(())
}
