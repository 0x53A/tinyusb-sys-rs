use cc::Build;
use std::fs::File;
use std::io::prelude::*;
use std::{
    env,
    path::{Path, PathBuf},
};

mod tusb_config;

fn add_all_c_files_in_dir(build: &mut Build, path: impl AsRef<Path>) {
    for entry in glob::glob(path.as_ref().join("**/*.c").to_str().unwrap()).unwrap() {
        let path = entry.unwrap();
        if path.extension().and_then(|s| s.to_str()) == Some("c") {
            build.file(&path);
        }
    }
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Missing OUT_DIR"));
    let target = env::var("TARGET").expect("Missing TARGET env var");

    {
        let mut f =
            File::create(out_dir.join("tusb_config.h")).expect("Failed to create tusb_config.h");
        f.write_all(tusb_config::generate_cfg().as_bytes())
            .expect("Failed to write to tusb_config.h");
    }

    // Hardcode toolchain paths (equivalent to exporting the env vars)
    // BINDGEN_EXTRA_CLANG_ARGS provides additional -I for bindgen
    // LIBCLANG_PATH tells the runtime where to find libclang
    env::set_var(
        "BINDGEN_EXTRA_CLANG_ARGS",
        "-I/home/lukas/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/xtensa-esp-elf/include",
    );
    env::set_var(
        "LIBCLANG_PATH",
        "/home/lukas/.rustup/toolchains/esp/xtensa-esp32-elf-clang/esp-19.1.2_20250225/esp-clang/lib",
    );

    // Set cross-compiler env vars early so cc::Build probes use the xtensa toolchain
    if let Ok(target) = env::var("TARGET") {
        if target == "xtensa-esp32s3-none-elf" {
            env::set_var(
                "CC_xtensa_esp32s3_none_elf",
                "/home/lukas/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin/xtensa-esp32s3-elf-gcc",
            );
            env::set_var(
                "AR_xtensa_esp32s3_none_elf",
                "/home/lukas/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin/xtensa-esp32s3-elf-ar",
            );
        }
    }

    let include_paths = String::from_utf8(
        Build::new()
            .get_compiler()
            .to_command()
            .arg("-E")
            .arg("-Wp,-v")
            .arg("-xc")
            .arg("/dev/null")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to run the compiler to get paths")
            .wait_with_output()
            .expect("Failed to run the compiler to get paths")
            .stderr,
    )
    .unwrap()
    .lines()
    .filter_map(|line| line.strip_prefix(" "))
    .map(|path| format!("-I{}", path))
    .collect::<Vec<_>>();

    eprintln!("include_paths={:?}", include_paths);

    let mut build = Build::new();
    add_all_c_files_in_dir(&mut build, "../tinyusb/src");
    build.flag("-mlongcalls");
    build.flag_if_supported("-Os");
    build
        .include("../tinyusb/src")
        .include(&out_dir) // for the tusb_config.h file
        .compile("tinyusb");

    
    // Set the correct cross-compiler for cc crate
    if target == "xtensa-esp32s3-none-elf" {
        env::set_var("CC_xtensa_esp32s3_none_elf", "xtensa-esp32s3-elf-gcc");
    }
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings = bindgen::Builder::default()
        .header("../tinyusb/src/tusb.h")
        .rustified_enum(".*")
        .clang_arg(&format!("-I{}", &out_dir.display()))
        .derive_default(true)
        .layout_tests(false)
        .use_core()
        .rustfmt_bindings(true)
        .ctypes_prefix("cty")
        .clang_args(&vec![
            "-target",
            &target,
            "-fvisibility=default",
            "-fshort-enums",
        ])
        .clang_arg("-I../tinyusb/src")
        .clang_args(&include_paths)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Can't write bindings!");
}
