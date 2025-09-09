use cc::Build;
use std::fs::File;
use std::io::prelude::*;
use std::{
    env,
    path::{Path, PathBuf},
};

mod tusb_config;

const TINUSB_PATH: &str = "./tinyusb";

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

    // We can assume that the user has installed the esp toolchain using 'espup'.

    if let Ok(target) = env::var("TARGET") {
        if target == "xtensa-esp32s3-none-elf" {
            let mut home = env::var("HOME").expect("Missing HOME env var");
            if home.ends_with('/') {
                home.pop();
            }

            let xtensa_esp_elf = format!(
                "{home}/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf"
            );
            let esp_clang = format!("{home}/.rustup/toolchains/esp/xtensa-esp32-elf-clang/esp-19.1.2_20250225/esp-clang");

            env::set_var(
                "BINDGEN_EXTRA_CLANG_ARGS",
                format!("-I{xtensa_esp_elf}/xtensa-esp-elf/include"),
            );
            env::set_var("LIBCLANG_PATH", format!("{esp_clang}/lib"));

            env::set_var(
                "CC_xtensa_esp32s3_none_elf",
                format!("{xtensa_esp_elf}/bin/xtensa-esp32s3-elf-gcc"),
            );
            env::set_var(
                "AR_xtensa_esp32s3_none_elf",
                format!("{xtensa_esp_elf}/bin/xtensa-esp32s3-elf-ar"),
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
    add_all_c_files_in_dir(&mut build, format!("{TINUSB_PATH}/src"));
    build.flag("-mlongcalls");
    build.flag_if_supported("-Os");
    build
        .include(format!("{TINUSB_PATH}/src"))
        .include(&out_dir) // for the tusb_config.h file
        .compile("tinyusb");

    // Set the correct cross-compiler for cc crate
    if target == "xtensa-esp32s3-none-elf" {
        env::set_var("CC_xtensa_esp32s3_none_elf", "xtensa-esp32s3-elf-gcc");
    }

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings = bindgen::Builder::default()
        .header(format!("{TINUSB_PATH}/src/tusb.h"))
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
        .clang_arg(format!("-I{TINUSB_PATH}/src"))
        .clang_args(&include_paths)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Can't write bindings!");
}
