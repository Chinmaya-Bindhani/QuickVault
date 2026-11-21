use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/quickvault.rc");
    println!("cargo:rerun-if-changed=assets/quickvault.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(if msvc {
        "icon.res"
    } else {
        "icon.o"
    });
    let status = if msvc {
        Command::new("rc.exe")
            .arg("/nologo")
            .arg("/fo")
            .arg(&output)
            .arg("assets/quickvault.rc")
            .status()
    } else {
        Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()))
            .args(["-i", "assets/quickvault.rc", "-O", "coff", "-o"])
            .arg(&output)
            .status()
    }
    .expect("Install the Windows resource compiler (rc.exe or windres) and put it on PATH.");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", output.display());
}
