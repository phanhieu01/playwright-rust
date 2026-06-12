# driver-patch — rebrowser stealth overlay

`build.rs` downloads the official Playwright driver, then swaps the files under
`<DRIVER_VERSION>/lib/**` into the driver's `package/lib/**` before embedding the zip
(`patch_driver()` in `src/build.rs`). This strips the CDP / `Runtime.enable` automation
residue that detectors look for, without re-implementing the Node driver.

These files are [rebrowser-patches](https://github.com/rebrowser/rebrowser-patches)
(`lib.patch`, runtime-fix) **rebased by hand onto playwright-core 1.57.0** — the upstream
patch does not apply cleanly to 1.57.0 (version drift). The fix is on by default
(`REBROWSER_PATCHES_RUNTIME_FIX_MODE` unset ⇒ `addBinding` mode); set the env var to `0`
at driver runtime to disable.

Patched entries (must mirror the driver's `package/lib/...` layout):

- `lib/server/chromium/crConnection.js` — adds `__re__emitExecutionContext` /
  `__re__getMainWorld` / `__re__getIsolatedWorld`
- `lib/server/chromium/crDevTools.js` — gate `Runtime.enable`
- `lib/server/chromium/crPage.js` — gate `Runtime.enable` (page + worker), thread
  `targetId`/`session` into `Worker`
- `lib/server/chromium/crServiceWorker.js` — gate `Runtime.enable`
- `lib/server/frames.js` — lazy main/utility-world acquisition in `_context`
- `lib/server/page.js` — `Worker.getExecutionContext`, `PageBinding.dispatch` guard

## Regenerating for a new DRIVER_VERSION

1. `npm i playwright-core@<ver>` in a scratch dir.
2. `npx rebrowser-patches@latest patch --packagePath node_modules/playwright-core`
   (if it fails on version drift, apply with `patch -p1 --fuzz=3 -l` and hand-fix the
   rejected hunks — watch for private fields that lost their leading `_`).
3. `node --check` every modified file.
4. Copy the 6 modified `lib/...` files here under `driver-patch/<ver>/lib/...` and bump
   `DRIVER_VERSION` in `src/build.rs`.
5. Verify against `bot-detector.rebrowser.net` (`runtimeEnableLeak` / `pwInitScripts`
   must stay green) — see `examples/hades_rebrowser_detector.rs`.
