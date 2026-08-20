use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn compile_shader(src: &Path, out: &Path) -> anyhow::Result<()> {
    let stage = match src.extension().and_then(|e| e.to_str()) {
        Some("vert") => "vert",
        Some("frag") => "frag",
        Some("geom") => "geom",
        other => anyhow::bail!("unknown shader stage for {:?}: {:?}", src, other),
    };
    let status = Command::new("glslc")
        .arg("-O")
        .arg(src)
        .arg("-o")
        .arg(out)
        .status()?;
    if !status.success() {
        anyhow::bail!("glslc failed for {:?}", src);
    }
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", out.display());
    let _ = stage;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let shader_dir = PathBuf::from(&manifest).join("shaders");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let spirv_dir = out_dir.join("spirv");
    fs::create_dir_all(&spirv_dir)?;

    for entry in fs::read_dir(&shader_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| {
            matches!(e.to_str(), Some("vert" | "frag" | "geom"))
        }) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap()
                .replace('.', "_");
            let out = spirv_dir.join(format!("{}.spv", name));
            compile_shader(&path, &out)?;
        }
    }
    println!("cargo:rustc-env=WSARNDR_SHADERS={}", spirv_dir.display());
    Ok(())
}