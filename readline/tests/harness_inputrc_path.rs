#[cfg(unix)]
#[test]
fn inputrc_file_environment_preserves_non_utf8_path_bytes() {
    use std::os::unix::ffi::OsStrExt;
    use std::process::{Command, Stdio};

    let directory = tempfile::tempdir().expect("create test directory");
    let path = directory
        .path()
        .join(std::ffi::OsStr::from_bytes(b"missing-\xff.inputrc"));
    let output = Command::new(env!("CARGO_BIN_EXE_sushline-harness"))
        .env_clear()
        .env("SUSHLINE_INPUTRC_FILE", &path)
        .stdin(Stdio::null())
        .output()
        .expect("run harness");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stderr.starts_with(b"SUSHLINE_INPUTRC_FILE_ERROR:"),
        "harness must attempt to load a non-UTF-8 inputrc path"
    );
}
