use std::{fs, path::PathBuf};

#[test]
fn test_merge() {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("tests/dbfiles");

    let files: Vec<String> = fs::read_dir(d)
        .unwrap()
        .filter_map(|res| res.ok())
        .map(|dir| dir.path())
        .filter(|f| f.extension().map_or(false, |ext| ext == "db"))
        .map(|f| f.display().to_string())
        .collect();

    for _ in 0..100 {
        let result = prometheus_client_python_speedups::merge_internal(&files);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
