use std::env;


fn main(){
    let target = env::var("TARGET").unwrap();

    if target == "i686-unknown-linux-gnu" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,.");
        println!("cargo:rustc-link-arg=-Wl,-rpath,bin/");
        println!("cargo:rustc-link-arg=-Wl,-rpath,garrysmod/bin/");
        println!("cargo:rustc-link-search=native=external/");
        println!("cargo:rustc-link-arg=-l:lua_shared_srv.so");
    }
}