// Build script to compile Swift ScreenCaptureKit bridge on macOS

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Only build Swift bridge on macOS
    if cfg!(target_os = "macos") {
        build_swift_bridge();
    }
}

#[cfg(target_os = "macos")]
fn build_swift_bridge() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let swift_src = "src/screencapture/bridge.swift";

    println!("cargo:rerun-if-changed={}", swift_src);

    // Compile Swift to object file
    let obj_file = PathBuf::from(&out_dir).join("bridge.o");

    let output = Command::new("swiftc")
        .args(&[
            "-emit-object",
            "-o",
            obj_file.to_str().unwrap(),
            swift_src,
            "-target",
            "arm64-apple-macosx13.0",  // Require macOS 13.0+
            "-sdk",
            "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk",
            "-parse-as-library",
            "-O",  // Optimize
        ])
        .output()
        .expect("Failed to execute swiftc");

    if !output.status.success() {
        panic!(
            "Swift compilation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Create static library from object file
    let lib_file = PathBuf::from(&out_dir).join("libloqa_screencapture.a");

    let output = Command::new("ar")
        .args(&["rcs", lib_file.to_str().unwrap(), obj_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute ar");

    if !output.status.success() {
        panic!(
            "Static library creation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Link the static library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=loqa_screencapture");

    // Link required system frameworks
    println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    println!("cargo:rustc-link-lib=framework=CoreAudio");
    println!("cargo:rustc-link-lib=framework=Foundation");

    // Link Swift runtime libraries
    // Find Swift toolchain path
    let swift_lib_path = "/usr/lib/swift";
    println!("cargo:rustc-link-search=native={}", swift_lib_path);

    // Add rpath for Swift runtime
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", swift_lib_path);
    println!("cargo:rustc-link-arg=-Wl,-rpath,/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx");

    println!("Swift bridge compiled successfully");
}

#[cfg(not(target_os = "macos"))]
fn build_swift_bridge() {
    // No-op on non-macOS platforms
}
