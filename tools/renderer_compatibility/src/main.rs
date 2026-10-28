//! Evidence runner using the same Naga version as the production renderer.
mod corpus;
mod gpu;
use std::{error::Error, fs, path::Path, process::Command};
fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("supply an output directory")?;
    fs::create_dir_all(&out)?;
    if let Some(path) = std::env::var_os("PROBE_CORPUS") {
        return corpus::run(Path::new(&path), Path::new(&out));
    }
    let mut failures = 0;
    for name in [
        "atomics",
        "workgroup_nested",
        "shared_composite",
        "uniform_layout",
    ] {
        let input = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/{name}.wgsl")),
        )?;
        let module = naga::front::wgsl::parse_str(&input)?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)?;
        let mut options = naga::back::spv::Options::default();
        options.lang_version = if std::env::var_os("PROBE_SPIRV_15").is_some() {
            (1, 5)
        } else {
            (1, 0)
        };
        let words = naga::back::spv::write_vec(&module, &info, &options, None)?;
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let binary = Path::new(&out).join(format!("{name}.spv"));
        fs::write(&binary, bytes)?;
        let result = Command::new("spirv-val")
            .arg("--relax-block-layout")
            .arg("--target-env")
            .arg("vulkan1.3")
            .arg(&binary)
            .output()?;
        fs::write(
            Path::new(&out).join(format!("{name}.validation.log")),
            &result.stderr,
        )?;
        let assembly = Command::new("spirv-dis").arg(&binary).output()?;
        if !assembly.status.success() {
            return Err("SPIR-V disassembly failed".into());
        }
        fs::write(
            Path::new(&out).join(format!("{name}.spvasm")),
            assembly.stdout,
        )?;
        println!(
            "{name}: {} {}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        );
        if !result.status.success() {
            failures += 1;
        }
    }
    if failures > 0 {
        return Err(format!("{failures} compiler fixtures failed Vulkan validation").into());
    }
    gpu::verify()?;
    Ok(())
}
