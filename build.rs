use std::path::Path;
use std::io::Result;

fn main() -> Result<()> {
    // Tell Cargo to re-run this script if the proto files change
    println!("cargo:rerun-if-changed=protos/");
    
    // Define output path for the generated code
    let _out_dir = std::env::var("OUT_DIR").unwrap();
    
    // Generate code from proto files
    prost_build::compile_protos(
        &["src/protos/service.proto"],
        &["src/protos/"]
    )?;
    
    Ok(())
}