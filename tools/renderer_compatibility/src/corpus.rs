//! Comparative upstream fixture validation; preserves all failures for baseline comparison.
use std::{error::Error, fs, path::Path, process::Command};
pub fn run(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(output)?;
    let mut files: Vec<_> = fs::read_dir(input)?.collect::<Result<Vec<_>, _>>()?;
    files.sort_by_key(|entry| entry.file_name());
    let mut report = String::new();
    for entry in files {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("wgsl") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let result = translate(&path, output);
        let status = match result {
            Ok(()) => "PASS".into(),
            Err(error) => error.to_string().replace('\n', " "),
        };
        report.push_str(&format!("{name}\t{status}\n"));
    }
    fs::write(output.join("report.tsv"), report)?;
    Ok(())
}
fn translate(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let module = naga::front::wgsl::parse_str(&fs::read_to_string(input)?)?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)?;
    let words =
        naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)?;
    let binary = output
        .join(input.file_name().ok_or("missing filename")?)
        .with_extension("spv");
    fs::write(
        &binary,
        words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )?;
    let checked = Command::new("spirv-val")
        .args(["--relax-block-layout", "--target-env", "vulkan1.3"])
        .arg(&binary)
        .output()?;
    if checked.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&checked.stderr).into_owned().into())
    }
}
