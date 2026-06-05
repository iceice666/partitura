use std::{fs, path::Path};

#[test]
fn stdout_writes_stay_in_output_module_and_no_print_macros_are_used() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit(&root, &mut |path| {
        let body = fs::read_to_string(path).unwrap();
        if body.contains("println!") || body.contains("print!") {
            offenders.push(path.display().to_string());
        }
        if path.file_name().unwrap() != "output.rs" && body.contains("stdout()") {
            offenders.push(path.display().to_string());
        }
    });
    assert!(offenders.is_empty(), "{offenders:?}");
}

fn visit(path: &Path, f: &mut dyn FnMut(&Path)) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, f);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            f(&path);
        }
    }
}
