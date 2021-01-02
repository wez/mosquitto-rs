#[cfg(feature = "vendored-mosquitto")]
fn main() {
    let mut cfg = cc::Build::new();
    let target = std::env::var("TARGET").unwrap();

    cfg.include("mosquitto");
    cfg.include("mosquitto/include");
    cfg.include("mosquitto/deps");
    cfg.include("mosquitto/lib");
    cfg.files(&[
        "mosquitto/lib/actions.c",
        "mosquitto/lib/callbacks.c",
        "mosquitto/lib/connect.c",
        "mosquitto/lib/handle_auth.c",
        "mosquitto/lib/handle_connack.c",
        "mosquitto/lib/handle_disconnect.c",
        "mosquitto/lib/handle_ping.c",
        "mosquitto/lib/handle_pubackcomp.c",
        "mosquitto/lib/handle_publish.c",
        "mosquitto/lib/handle_pubrec.c",
        "mosquitto/lib/handle_pubrel.c",
        "mosquitto/lib/handle_suback.c",
        "mosquitto/lib/handle_unsuback.c",
        "mosquitto/lib/helpers.c",
        "mosquitto/lib/logging_mosq.c",
        "mosquitto/lib/loop.c",
        "mosquitto/lib/memory_mosq.c",
        "mosquitto/lib/messages_mosq.c",
        "mosquitto/lib/misc_mosq.c",
        "mosquitto/lib/mosquitto.c",
        "mosquitto/lib/net_mosq.c",
        "mosquitto/lib/net_mosq_ocsp.c",
        "mosquitto/lib/options.c",
        "mosquitto/lib/packet_datatypes.c",
        "mosquitto/lib/packet_mosq.c",
        "mosquitto/lib/property_mosq.c",
        "mosquitto/lib/read_handle.c",
        "mosquitto/lib/send_connect.c",
        "mosquitto/lib/send_disconnect.c",
        "mosquitto/lib/send_mosq.c",
        "mosquitto/lib/send_publish.c",
        "mosquitto/lib/send_subscribe.c",
        "mosquitto/lib/send_unsubscribe.c",
        "mosquitto/lib/socks_mosq.c",
        "mosquitto/lib/srv_mosq.c",
        "mosquitto/lib/strings_mosq.c",
        "mosquitto/lib/thread_mosq.c",
        "mosquitto/lib/time_mosq.c",
        "mosquitto/lib/tls_mosq.c",
        "mosquitto/lib/utf8_mosq.c",
        "mosquitto/lib/util_mosq.c",
        "mosquitto/lib/util_topic.c",
        "mosquitto/lib/will_mosq.c",
    ]);
    cfg.define("WITH_THREADING", None);
    cfg.define("WITH_UNIX_SOCKETS", None);
    if !target.contains("windows") {
        cfg.flag("-fvisibility=hidden");
    }
    cfg.warnings(false);

    println!("cargo:rerun-if-env-changed=DEP_OPENSSL_INCLUDE");
    if let Some(path) = std::env::var_os("DEP_OPENSSL_INCLUDE") {
        if let Some(path) = std::env::split_paths(&path).next() {
            if let Some(path) = path.to_str() {
                if path.len() > 0 {
                    cfg.include(path);
                    cfg.define("WITH_TLS", None);
                    cfg.define("WITH_TLS_PSK", None);
                    cfg.define("WITH_EC", None);
                    println!("cargo:rustc-link-lib=ssl");
                    println!("cargo:rustc-link-lib=crypto");
                }
            }
        }
    }

    cfg.compile("mosquitto");
}

#[cfg(not(feature = "vendored-mosquitto"))]
fn main() {
    if pkg_config::Config::new()
        .atleast_version("1.4")
        .probe("libmosquitto")
        .is_err()
    {
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
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
