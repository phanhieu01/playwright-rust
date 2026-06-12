use std::{
    env, fmt, fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf, MAIN_SEPARATOR},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const DRIVER_VERSION: &str = "1.57.0";

// Driver entries replaced by the rebrowser-patches overlay (rebased onto
// playwright-core 1.57.0) to strip the Runtime.enable / CDP-residue tells.
// The patched copies live in driver-patch/<DRIVER_VERSION>/lib/**.
// See driver-patch/README.md.
const PATCH_TARGETS: &[&str] = &[
    "package/lib/server/chromium/crConnection.js",
    "package/lib/server/chromium/crDevTools.js",
    "package/lib/server/chromium/crPage.js",
    "package/lib/server/chromium/crServiceWorker.js",
    "package/lib/server/frames.js",
    "package/lib/server/page.js",
];

fn main() {
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let dest = out_dir.join("driver.zip");
    let vanilla = out_dir.join("driver-vanilla.zip");
    let platform = PlaywrightPlatform::default();
    fs::write(out_dir.join("platform"), platform.to_string()).unwrap();
    download(&url(platform), &vanilla);
    patch_driver(&vanilla, &dest);
    println!("cargo:rerun-if-changed=src/build.rs");
    println!("cargo:rerun-if-changed=driver-patch");
    println!("cargo:rustc-env=SEP={}", MAIN_SEPARATOR);
}

/// Re-emit the driver zip with the rebrowser-patched `package/lib` files swapped
/// in. Everything else (node binary, cli.js, assets) is copied through verbatim.
fn patch_driver(vanilla: &Path, dest: &Path) {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let overlay_root: PathBuf = [manifest.as_str(), "driver-patch", DRIVER_VERSION, "lib"]
        .iter()
        .collect();

    let mut zin = ZipArchive::new(File::open(vanilla).unwrap()).unwrap();
    let mut zout = ZipWriter::new(File::create(dest).unwrap());
    let mut applied = 0usize;
    for i in 0..zin.len() {
        let mut entry = zin.by_index(i).unwrap();
        let name = entry.name().to_string();
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(entry.unix_mode().unwrap_or(0o644));
        if entry.is_dir() {
            zout.add_directory(name.trim_end_matches('/'), opts).unwrap();
            continue;
        }
        let bytes = if PATCH_TARGETS.contains(&name.as_str()) {
            let sub = name.strip_prefix("package/lib/").unwrap();
            let patched = overlay_root.join(sub);
            applied += 1;
            fs::read(&patched).unwrap_or_else(|e| {
                panic!(
                    "rebrowser overlay missing: {} ({e}). If DRIVER_VERSION ({}) changed, \
                     regenerate driver-patch/<ver>/ — see driver-patch/README.md",
                    patched.display(),
                    DRIVER_VERSION
                )
            })
        } else {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).unwrap();
            buf
        };
        zout.start_file(name, opts).unwrap();
        zout.write_all(&bytes).unwrap();
    }
    zout.finish().unwrap();
    assert_eq!(
        applied,
        PATCH_TARGETS.len(),
        "rebrowser overlay: expected {} target files in the driver zip, applied {}. \
         Driver layout changed for {}?",
        PATCH_TARGETS.len(),
        applied,
        DRIVER_VERSION
    );
}

#[cfg(all(not(feature = "only-for-docs-rs"), not(unix)))]
fn download(url: &str, dest: &Path) {
    let mut resp = reqwest::blocking::get(url).unwrap();
    let mut dest = File::create(dest).unwrap();
    resp.copy_to(&mut dest).unwrap();
}

#[cfg(all(not(feature = "only-for-docs-rs"), unix))]
fn download(url: &str, dest: &Path) {
    let cache_dir: &Path = "/tmp/build-playwright-rust".as_ref();
    let cached = cache_dir.join("driver.zip");
    if cfg!(debug_assertions) {
        let maybe_metadata = cached.metadata().ok();
        let cache_is_file = || {
            maybe_metadata
                .as_ref()
                .map(fs::Metadata::is_file)
                .unwrap_or_default()
        };
        let cache_size = || {
            maybe_metadata
                .as_ref()
                .map(fs::Metadata::len)
                .unwrap_or_default()
        };
        if cache_is_file() && cache_size() > 10000000 {
            fs::copy(cached, dest).unwrap();
            return;
        }
    }
    let mut resp = reqwest::blocking::get(url).unwrap();
    let mut dest_file = File::create(dest).unwrap();
    resp.copy_to(&mut dest_file).unwrap();
    if cfg!(debug_assertions) {
        fs::create_dir_all(cache_dir).unwrap();
        fs::copy(dest, cached).unwrap();
    }
}

#[allow(dead_code)]
fn size(p: &Path) -> u64 {
    let maybe_metadata = p.metadata().ok();
    let size = maybe_metadata
        .as_ref()
        .map(fs::Metadata::len)
        .unwrap_or_default();
    size
}

// No network access
#[cfg(feature = "only-for-docs-rs")]
fn download(_url: &str, dest: &Path) {
    File::create(dest).unwrap();
}

fn url(platform: PlaywrightPlatform) -> String {
    // For stable versions, no /next prefix is needed
    // /next is only for pre-release versions
    let next = if DRIVER_VERSION.contains("next")
        || DRIVER_VERSION.contains("alpha")
        || DRIVER_VERSION.contains("beta")
    {
        "/next"
    } else {
        ""
    };
    format!(
        "https://playwright.azureedge.net/builds/driver{}/playwright-{}-{}.zip",
        next, DRIVER_VERSION, platform
    )
}

#[derive(Clone, Copy)]
enum PlaywrightPlatform {
    Linux,
    Win32,
    Win32x64,
    Mac,
}

impl fmt::Display for PlaywrightPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Linux => write!(f, "linux"),
            Self::Win32 => write!(f, "win32"),
            Self::Win32x64 => write!(f, "win32_x64"),
            Self::Mac => write!(f, "mac"),
        }
    }
}

impl Default for PlaywrightPlatform {
    fn default() -> Self {
        match env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("linux") => return PlaywrightPlatform::Linux,
            Ok("macos") => return PlaywrightPlatform::Mac,
            _ => (),
        };
        if env::var("CARGO_CFG_WINDOWS").is_ok() {
            if env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() == Ok("64") {
                PlaywrightPlatform::Win32x64
            } else {
                PlaywrightPlatform::Win32
            }
        } else if env::var("CARGO_CFG_UNIX").is_ok() {
            PlaywrightPlatform::Linux
        } else {
            panic!("Unsupported plaform");
        }
    }
}
