use std::env;


fn main(){
    let target = env::var("TARGET").unwrap();
    
    println!("cargo:rustc-link-search=native=external/");

    if target == "i686-unknown-linux-gnu" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,.");
        println!("cargo:rustc-link-arg=-Wl,-rpath,bin/");
        println!("cargo:rustc-link-arg=-Wl,-rpath,garrysmod/bin/");
        println!("cargo:rustc-link-arg=-l:lua_shared_srv.so");
    }

    if target == "i686-pc-windows-msvc" {
        println!("cargo:rustc-link-lib=lua_shared");
    }
}