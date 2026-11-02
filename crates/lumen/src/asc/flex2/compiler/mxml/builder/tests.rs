//! Runs the whole MXML pipeline on `BUILDER_APP` (an `.mxml`) with keep-generated-actionscript into
//! `BUILDER_OUT`, for comparing the generated sources with mxmlc's.

use std::path::PathBuf;

use crate::asc::flex2::compiler::file_configurator;

const FLEXLIB: &str = r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk\frameworks";

#[test]
fn generate_app() {
    let (Ok(app), Ok(out)) = (std::env::var("BUILDER_APP"), std::env::var("BUILDER_OUT")) else { return };
    let mut opts = file_configurator::mxmlc_defaults(&PathBuf::from(FLEXLIB)).expect("flex-config.xml");
    let app = PathBuf::from(app);
    opts.main = app.clone();
    opts.source_path.push(app.parent().unwrap().to_path_buf());
    opts.generated_directory = Some(PathBuf::from(&out));
    opts.compile_date = Some(0);
    let result = crate::asc::flex2::compile_application(&opts);
    let diagnostics = match &result {
        Ok(a) => a.diagnostics.clone(),
        Err(d) => d.clone(),
    };
    let text: Vec<String> = diagnostics.iter().map(|d| d.text.clone()).collect();
    std::fs::write(PathBuf::from(&out).join("diagnostics.txt"), text.join("\n\n")).unwrap();
    if let Ok(a) = result {
        std::fs::write(PathBuf::from(&out).join("out.swf"), &a.swf).unwrap();
    }
}
