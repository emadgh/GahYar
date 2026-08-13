use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=icon/GahYar.ico");
    println!("cargo:rerun-if-changed=fonts/Vazirmatn-Regular.ttf");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return,
    };
    let Some(rc) = find_resource_compiler(arch) else {
        panic!("Windows resource compiler (rc.exe) was not found");
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("GahYar.res");
    let status = Command::new(rc)
        .args(["/nologo", "/fo"])
        .arg(&output)
        .arg("app.rc")
        .status()
        .expect("failed to start rc.exe");
    assert!(status.success(), "rc.exe failed to compile app.rc");
    println!("cargo:rustc-link-arg={}", output.display());
}

fn find_resource_compiler(arch: &str) -> Option<PathBuf> {
    if let Some(sdk_dir) = env::var_os("WindowsSdkDir") {
        let bin = PathBuf::from(sdk_dir).join("bin");
        if let Some(found) = newest_rc(&bin, arch) {
            return Some(found);
        }
    }
    newest_rc(
        Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin"),
        arch,
    )
}

fn newest_rc(bin: &Path, arch: &str) -> Option<PathBuf> {
    let direct = bin.join(arch).join("rc.exe");
    if direct.is_file() {
        return Some(direct);
    }
    let mut versions = fs::read_dir(bin)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort();
    versions
        .into_iter()
        .rev()
        .map(|version| version.join(arch).join("rc.exe"))
        .find(|path| path.is_file())
}
