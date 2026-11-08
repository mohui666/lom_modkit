use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=LOM_EDITOR_COMPONENTS");
    let generated = if let Some(path) = env::var_os("LOM_EDITOR_COMPONENTS") {
        let path = PathBuf::from(path)
            .canonicalize()
            .expect("embedded components archive");
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).expect("read embedded components");
        let key = format!("{:x}", Sha256::digest(&bytes));
        format!(
            "const PAYLOAD: &[u8] = include_bytes!({:?});\nconst KEY: &str = {:?};\n",
            path, key
        )
    } else {
        "const PAYLOAD: &[u8] = &[];\nconst KEY: &str = \"\";\n".into()
    };
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("components.rs"),
        generated,
    )
    .unwrap();
}
