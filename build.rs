// use std::env;


// fn main(){
//     let target = env::var("TARGET").unwrap();

//     if target == "i686-unknown-linux-gnu" {
//         println!("cargo:rustc-link-arg=-Wl,-rpath,.");
//         println!("cargo:rustc-link-arg=-Wl,-rpath,bin/");
//         println!("cargo:rustc-link-arg=-Wl,-rpath,garrysmod/bin/");
//         println!("cargo:rustc-link-search=native=external/");
//         println!("cargo:rustc-link-arg=-l:lua_shared_srv.so");
//     }
// }
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    let triple = &env::var("TARGET")?;
    let mut target = triple.split("-");
    let arch = target.next().unwrap_or("x86_64");
    let search_paths = match arch {
        "i686" => &[".", "bin/", "bin/linux32/", "garrysmod/bin/"][..],
        "x86_64" => &[".", "bin/linux64/", "linux64"][..],
        _ => &[][..],
    };
    for search_path in search_paths {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", search_path);
        println!("cargo:warning={:?}", search_path);
    }
    Ok(())
}