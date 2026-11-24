use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn gnu_manifest_override(out: &Path) {
    // Some MinGW distributions add their own RT_MANIFEST through GCC's endfile
    // spec. Keep the CRT objects and omit just that resource so our manifest wins.
    let linker = env::var_os("RUSTC_LINKER").unwrap_or_else(|| "x86_64-w64-mingw32-gcc".into());
    let Ok(specs) = Command::new(linker).arg("-dumpspecs").output() else {
        return;
    };
    if !specs.status.success() {
        return;
    }
    let specs = String::from_utf8_lossy(&specs.stdout);
    let original = specs
        .lines()
        .skip_while(|line| *line != "*endfile:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let without_manifest = original.replace("%{!shared:%:if-exists(default-manifest.o%s)}", "");
    if without_manifest != original {
        let path = out.join("app-endfile.specs");
        fs::write(&path, format!("*endfile:\n{without_manifest}\n\n")).unwrap();
        println!("cargo:rustc-link-arg-bins=-specs={}", path.display());
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/performance-glass.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=RUSTC_LINKER");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let target = env::var("TARGET").unwrap();
    assert!(
        matches!(
            target.as_str(),
            "x86_64-pc-windows-gnu" | "x86_64-pc-windows-msvc"
        ),
        "Build for a supported x64 Windows target"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::copy("assets/icon/performance-glass.ico", out.join("app.ico"))
        .expect("Application icon is required");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric_version = format!("{},0", version.replace('.', ","));
    let manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="PerformanceGlass.Native" version="{version}.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#
    );
    fs::write(out.join("app.manifest"), manifest).unwrap();
    let resource = format!(
        r#"1 24 "app.manifest"
101 ICON "app.ico"
102 DIALOGEX 0, 0, 160, 140
STYLE 0x80C808C0
FONT 9, "Segoe UI"
BEGIN
  LTEXT "", 1001, 20, 18, 120, 12
  LTEXT "", 1002, 20, 36, 120, 12
  CONTROL "", 1003, "BUTTON", 0x0001000B, 20, 60, 126, 16
  CONTROL "", 1004, "BUTTON", 0x0001000B, 20, 79, 126, 16
  CONTROL "", 2, "BUTTON", 0x0001000B, 63, 108, 78, 22
END
1 VERSIONINFO
FILEVERSION {numeric_version}
PRODUCTVERSION {numeric_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 1
FILESUBTYPE 0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "nekomimiyks\0"
      VALUE "FileDescription", "Performance Glass performance monitor\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "PerformanceGlass\0"
      VALUE "OriginalFilename", "PerformanceGlass.exe\0"
      VALUE "ProductName", "Performance Glass\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    );
    fs::write(out.join("app.rc"), resource).unwrap();
    if target.ends_with("-msvc") {
        let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
        let status = Command::new(compiler)
            .current_dir(&out)
            .args(["/nologo", "/fo", "app-resources.res", "app.rc"])
            .status()
            .expect("rc.exe is required: use Windows SDK or set RC");
        assert!(
            status.success(),
            "Failed to compile Windows application resources"
        );
        println!(
            "cargo:rustc-link-arg-bins={}",
            out.join("app-resources.res").display()
        );
        return;
    }
    let compiler = PathBuf::from(env::var_os("WINDRES").unwrap_or_else(|| "windres.exe".into()));
    let mut command = Command::new(&compiler);
    if let Some(directory) = compiler.parent().filter(|p| !p.as_os_str().is_empty()) {
        let gcc = directory.join("gcc.exe");
        if gcc.is_file() {
            command.arg("--preprocessor").arg(gcc);
            command.args([
                "--preprocessor-arg=-E",
                "--preprocessor-arg=-xc",
                "--preprocessor-arg=-DRC_INVOKED",
            ]);
        }
        let mut paths = vec![directory.to_path_buf()];
        paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
        command.env("PATH", env::join_paths(paths).unwrap());
    }
    command.current_dir(&out).args([
        "--input",
        "app.rc",
        "--output",
        "app-resources.o",
        "--output-format",
        "coff",
        "--target",
        "pe-x86-64",
    ]);
    let status = command
        .status()
        .expect("windres.exe is required: add MinGW bin to PATH or set WINDRES");
    assert!(
        status.success(),
        "Failed to compile Windows application resources"
    );
    println!(
        "cargo:rustc-link-arg-bins={}",
        out.join("app-resources.o").display()
    );
    gnu_manifest_override(&out);
}
