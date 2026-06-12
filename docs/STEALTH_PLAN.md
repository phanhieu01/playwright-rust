# Hades × playwright-rust — Stealth / CDP-residue plan

Goal: when `playwright-rust` drives the Hades anti-detect browser, leave no automation residue that a
detector page can see. Drafted for a fresh session to pick up and execute.

## 1. Architecture (verified — read this first)

`playwright-rust` is **NOT** a from-scratch CDP client. It is a typed Rust client over the **official
Playwright Node driver**:

- `src/build.rs` downloads the official driver at build time. **`DRIVER_VERSION = "1.57.0"`** → writes
  `driver.zip` into `OUT_DIR`.
- `src/imp/core/driver.rs` `include_bytes!`s that zip into the binary, extracts it at runtime to the
  cache dir, and spawns `node package/cli.js run-driver`.
- `src/api/*` + `src/imp/*` just speak Playwright's JSON driver protocol over a pipe.

**Consequence:** all the injection mechanics that leave residue — `Runtime.enable`, execution-world
setup, binding names, init scripts — live inside the bundled **`playwright-core` JS** (the `package/`
dir of the zip), **not** in the Rust code. So stealth fixes happen at one of three layers, in order of
impact:
- (A) **patch the bundled `playwright-core`** (the real fix),
- (B) **bypass via raw CDP** using the new `CdpSession` API (`page.cdp_session().send(...)`),
- (C) **Hades browser-side** patch as defense-in-depth.

`navigator.webdriver` is already handled browser-side (Hades M6) — don't re-do it here.

## 2. What actually leaks (priority order)

1. **Runtime.Enable leak** (highest). Playwright calls `Runtime.enable` to acquire execution contexts.
   Detectors flag that a CDP client enabled Runtime. (The *getter* side-channel is already safe in Hades
   M6; this is about the *call itself* + how contexts are obtained.)
2. **Main-world execution-context leak** — tied to the Runtime fix; how evaluate() acquires the main
   world without Runtime.enable.
3. **Binding-name residue** — `__playwright__binding__` / `__pwInitScripts` / exposeBinding names,
   visible via `Object.getOwnPropertyNames(window)`.
4. **Injected `//# sourceURL=` leak** — recognizable source URLs on injected scripts.
5. **Utility-world name** (`__playwright_utility_world__` etc.).

## 3. Canonical fix: rebrowser-patches on the bundled driver

`rebrowser-patches` (github.com/rebrowser/rebrowser-patches) is the well-known patch set for EXACTLY
these leaks; it patches `playwright-core`. It is versioned — confirm it covers **1.57.0** (or rebase the
nearest variant by hand). Flags it introduces:
- `REBROWSER_PATCHES_RUNTIME_FIX_MODE` (e.g. `addBinding` / `alwaysIsolated`)
- `REBROWSER_PATCHES_SOURCE_URL`
- `REBROWSER_PATCHES_UTILITY_WORLD_NAME`

## 4. Integration point in playwright-rust

The patch must change the `package/` JS **before** `driver.zip` is embedded by `build.rs`.

- **Option A (recommended first cut): pre-patched driver.zip.**
  1. Scratch dir: `npm i playwright-core@1.57.0`.
  2. `npx rebrowser-patches@latest patch --packagePath node_modules/playwright-core`.
  3. Build the patched zip with the SAME layout as the official one: download the official
     `driver.zip`, unzip, overlay the patched `package/lib/**` files, re-zip (preserve `node` binary +
     exec bits + structure).
  4. Point `build.rs` at the patched zip (local path or your own hosted URL) instead of the CDN.
- **Option B (reproducible, later): build.rs post-download hook.** After `download()`: extract → apply a
  checked-in `.patch` to `package/lib` (or shell out to rebrowser) → re-zip. Keeps it in-tree but adds
  Node/npm to the build toolchain.

Start with A (deterministic, simplest), graduate to B for reproducibility.

## 5. Verification — measure with the REAL client attached

Residue only appears once `playwright-rust` is driving the browser. So:
1. Use `playwright-rust` to launch / `connectOverCDP` the Hades `chrome.exe`.
2. In the page, run the detector battery:
   - `C:\hb\e2e\botdetect.js` (already exists) — `cdpGlobals` must stay `[]`, webdriver false.
   - **rebrowser bot-detector** (bot-detector.rebrowser.net) — checks dummyFn / exposeFunction /
     sourceUrlLeak / mainWorldExecution / runtimeEnableLeak.
   - CreepJS stealth section.
3. Record **before vs after** the driver patch: `runtimeEnableLeak`, `mainWorldExecution`,
   `sourceUrlLeak` should flip green.

## 6. Risks / gotchas

- **Version drift:** rebrowser may not apply cleanly to 1.57.0 → manual hunk rebase likely.
- **Zip fidelity:** re-zipping must preserve the `node` exec bit + exact layout or driver extraction /
  spawn breaks. Test `Driver::install()` + a smoke `page.goto` after repackaging.
- **Behavior change:** the runtime-fix mode changes how main-world `evaluate()` works → run the existing
  `tests/` to confirm no regressions.
- **Leftovers → Hades (C):** anything rebrowser doesn't cover, patch browser-side in
  `content/browser/devtools/` (e.g. the `Runtime.addBinding` / `Page.addScriptToEvaluateOnNewDocument`
  handlers) so residue is hidden regardless of client.

## 7. First-session checklist

- [ ] Build once, grab `driver.zip` from `OUT_DIR`, unzip; locate in `package/lib`: the `Runtime.enable`
      call sites (`server/chromium/*`), the binding-name constant, the injected `sourceURL`.
- [ ] Get rebrowser-patches; apply to `playwright-core@1.57.0`; `git diff` to see exactly what changes.
- [ ] Produce the patched `driver.zip`; wire `build.rs` to it.
- [ ] `cargo build`; smoke test (launch Hades, `page.goto`, `evaluate`).
- [ ] Run rebrowser bot-detector + botdetect.js before/after; record deltas.
- [ ] File browser-side Hades patches for any residue rebrowser leaves.
