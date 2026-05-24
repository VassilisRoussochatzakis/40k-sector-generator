use camino::Utf8PathBuf;
use include_dir::{include_dir, Dir};
use std::fs;
use tempfile::TempDir;

pub static EXAMPLES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples");

pub fn list_bundled_examples() -> Vec<String> {
    let mut out = Vec::new();
    for entry in EXAMPLES_DIR.dirs() {
        let name = entry.path().file_name().unwrap().to_str().unwrap();
        if name == "big_test" || name == "big_sparse_test" || name == "m42_project" {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

pub fn extract_example(name: &str) -> Result<(TempDir, Utf8PathBuf), String> {
    let dir = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let target_path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    let example_dir = EXAMPLES_DIR
        .get_dir(name)
        .ok_or_else(|| format!("example '{name}' not found in bundle"))?;

    extract_dir_recursive(example_dir, &target_path)?;

    Ok((dir, target_path))
}

pub fn extract_example_to_dir(name: &str, target_path: &Utf8PathBuf) -> Result<(), String> {
    let example_dir = EXAMPLES_DIR
        .get_dir(name)
        .ok_or_else(|| format!("example '{name}' not found in bundle"))?;

    extract_dir_recursive(example_dir, target_path)?;

    Ok(())
}

fn extract_dir_recursive(dir: &Dir, target: &Utf8PathBuf) -> Result<(), String> {
    fs::create_dir_all(target.as_std_path())
        .map_err(|e| format!("failed to create dir {target}: {e}"))?;

    for entry in dir.files() {
        let file_path = target.join(entry.path().file_name().unwrap().to_str().unwrap());
        fs::write(file_path.as_std_path(), entry.contents())
            .map_err(|e| format!("failed to write file {file_path}: {e}"))?;
    }

    for child in dir.dirs() {
        let child_name = child.path().file_name().unwrap().to_str().unwrap();
        let child_target = target.join(child_name);
        extract_dir_recursive(child, &child_target)?;
    }

    Ok(())
}
