fn main() {
    println!("cargo:rustc-link-lib=dylib=c");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");

    #[cfg(target_arch = "x86_64")]
    {
        println!("cargo:rustc-link-search=native=/lib/x86_64-linux-gnu");
        println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
    }
}