fn main() {
    pkg_config::Config::new()
        .atleast_version("1.6")
        .probe("libmosquitto")
        .unwrap();
}
