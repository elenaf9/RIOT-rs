use std::env;
use std::fs;
use std::path::Path;

fn arch_cfg() {
    let target = env::var("TARGET").unwrap();

    if target.starts_with("thumbv6m") {
        println!("cargo:rustc-cfg=armv6m")
    }

    if target.starts_with("thumbv7m")
        | target.starts_with("thumbv7em")
        | target.starts_with("thumbv8m")
    {
        println!("cargo:rustc-cfg=armv7m")
    }
}

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    let out_path = Path::new(&out_dir);
    let gen_include_path = out_path.join("include");
    let include_path = Path::new(&crate_dir).join("include");

    // generate C header for C bindings
    let gen_header = gen_include_path.join("ariel-os-core.h");

    std::fs::create_dir_all(&gen_include_path).unwrap();

    cbindgen::generate(&crate_dir)
        .expect("Unable to generate bindings")
        .write_to_file(&*gen_header);

    // generate RIOT makefile snippet
    let makefile_content = format!(
        "export USE_RUST_CORE = 1\n\
         DISABLE_MODULE += core\n\
         USEMODULE += ariel_os_core\n\
         PSEUDOMODULE += ariel_os_core\n\
         INCLUDES += -I{}\n\
         INCLUDES += -I{}\n",
        gen_include_path.to_string_lossy(),
        include_path.to_string_lossy()
    );

    let makefile_name = "Makefile.ariel-os-core";
    fs::write(out_path.join(&makefile_name), &makefile_content)
        .expect("Couldn't write ariel-os-core makefile!");

    // let dependent crates know the location of our makefile snippet
    // This requires `links = "riot-core-rs"` in Cargo.toml of this package.
    println!(
        "cargo:MAKEFILE={}",
        out_path.join(&makefile_name).to_string_lossy()
    );

    // set target specific config values
    arch_cfg();

    // to make sure this script is re-run on binding changes,
    // list cbindgen.toml and all .rs that contain c bindings
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src");
}
