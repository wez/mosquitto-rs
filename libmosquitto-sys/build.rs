use std::env;
use std::path::PathBuf;

fn main() {
    if std::env::var_os("DOCS_RS").is_some() {
        eprintln!("WARNING: DOCS_RS is set in the environment, NOT linking to libmosquitto");
        return;
    }

    if pkg_config::Config::new()
        .atleast_version("1.4")
        .probe("libmosquitto")
        .is_err()
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let file_name = out_dir.join("m.c");

        std::fs::write(
            &file_name,
            b"
        #include <mosquitto.h>
        int testing_mosquitto_linkage(void) {
          mosquitto_lib_init();
          return 0;
        }
        ",
        )
        .unwrap();
        println!("cargo:rustc-link-lib=mosquitto");

        let mut cfg = cc::Build::new();
        cfg.file(file_name);
        cfg.compile("testing");
    }
}
