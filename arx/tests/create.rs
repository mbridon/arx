use std::io::Read;

#[cfg(all(unix, not(feature = "in_ci")))]
mod inner {
    pub use std::path::Path;

    // Generate a fake directory with fake content.
    pub fn spawn_mount() -> std::io::Result<(arx_test_dir::BackgroundSession, tempfile::TempDir)> {
        let mount_path =
            tempfile::TempDir::with_prefix_in("source_", env!("CARGO_TARGET_TMPDIR")).unwrap();
        let builder = arx_test_dir::ContextBuilder::new();
        let context = builder.create();
        let dir = arx_test_dir::DirEntry::new_root(context);
        let mount_dir = arx_test_dir::TreeFs::new(dir);
        Ok((mount_dir.spawn(mount_path.path())?, mount_path))
    }
}

macro_rules! cmd {
    ("{cmd}", $command:ident, $arg:expr) => {{
        $command.arg($arg);
        $command.output().expect("Launching arx command should work.")
    }};
    ("{cmd}", $command:ident, $arg:expr, $($args:expr),+) => {{
        $command.arg($arg);
        cmd!("{cmd}", $command, $($args),+)
    }};
    ("arx", $sub_command:literal, $($args:expr),*) => {{
        let arx_bin = env!("CARGO_BIN_EXE_arx");
        let mut command = std::process::Command::new(&arx_bin);
        command.env("NO_COLOR", "1");
        cmd!("{cmd}", command, $sub_command, $($args),*)
    }};
    ($prog:literal, $($args:expr),*) => {{
        let mut command = std::process::Command::new($prog);
        cmd!("{cmd}", command, $($args),*)
    }};
}

#[test]
fn test_crate_non_existant_input() {
    use std::path::Path;

    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!("arx", "create", "--outfile", &arx_file, "non_existant_dir");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    println!("Out : {}", stdout);
    println!("Err : {}", stderr);
    assert_eq!("", stdout);
    assert_eq!(
        "[ERROR arx] Error : Input non_existant_dir path doesn't exist or cannot be accessed\n",
        stderr
    );
    assert!(!output.status.success());
    assert!(!arx_file.exists());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_crate_non_existant_output_directory() {
    use inner::*;
    use std::path::Path;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir
        .path()
        .join("non_existant_directory")
        .join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    println!("Out : {}", stdout);
    println!("Err : {}", stderr);
    assert_eq!("", stdout);
    assert_eq!(
        format!(
            "[ERROR arx] Error : Directory {} doesn't exist\n",
            arx_file.parent().unwrap().display()
        ),
        stderr
    );
    assert!(!output.status.success());
    assert!(!arx_file.exists());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_crate_existant_output() {
    use inner::*;
    use std::path::Path;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&arx_file).unwrap();
        f.write_all(b"Some dummy content").unwrap();
    }

    // Try to write without --force
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    println!("Out : {}", stdout);
    println!("Err : {}", stderr);
    assert_eq!("", stdout);
    assert_eq!(
        format!(
            "[ERROR arx] Error : File {} already exists. Use option --force to overwrite it.\n",
            arx_file.display()
        ),
        stderr
    );
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&arx_file).unwrap(), b"Some dummy content");

    // Try to write without --force
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap(),
        "--force"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    println!("Out : {}", stdout);
    println!("Err : {}", stderr);
    assert_eq!("", stdout);
    assert_eq!("", stderr);
    assert!(output.status.success());
    {
        let mut f = std::fs::File::open(&arx_file).unwrap();
        let mut buf = [0; 10];
        f.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"jbkC\x00\x00\x00\x00\x00\x02");
    }
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_create_and_mount() {
    use inner::*;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err : {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
    assert!(arx_file.is_file());

    let mount_point = tempfile::TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let arx = arx::Arx::new(arx_file).unwrap();
    let arxfs = arx::ArxFs::new(arx).unwrap();
    let _mount_handle = arxfs
        .spawn_mount("Test mounted arx".into(), mount_point.path())
        .unwrap();
    let output = cmd!("diff", "-r", source_mount_point, mount_point.path());
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err: {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_create_and_extract() {
    use inner::*;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err : {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
    assert!(arx_file.is_file());

    let extract_dir = tempfile::TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    arx::extract(
        &arx_file,
        extract_dir.path(),
        Default::default(),
        true,
        false,
    )
    .unwrap();
    let output = cmd!("diff", "-r", source_mount_point, extract_dir.path());
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err: {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_create_and_extract_filter() {
    use inner::*;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err : {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
    assert!(arx_file.is_file());

    let extract_dir =
        tempfile::TempDir::with_prefix_in("extract_", env!("CARGO_TARGET_TMPDIR")).unwrap();
    arx::extract(
        &arx_file,
        extract_dir.path(),
        ["OrcBlIw".into()].into(),
        true,
        true,
    )
    .unwrap();

    let mut source_sub_dir = source_mount_point.to_path_buf();
    source_sub_dir.push("OrcBlIw");
    let mut extract_sub_dir = extract_dir.path().to_path_buf();
    extract_sub_dir.push("OrcBlIw");

    println!(
        "Diff {} and {}",
        source_sub_dir.display(),
        extract_sub_dir.display()
    );
    let output = cmd!("diff", "-r", &source_sub_dir, &extract_sub_dir);
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err: {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_create_and_extract_subdir() {
    use inner::*;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err : {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
    assert!(arx_file.is_file());

    let extract_dir =
        tempfile::TempDir::with_prefix_in("extract_", env!("CARGO_TARGET_TMPDIR")).unwrap();

    let output = cmd!(
        "arx",
        "extract",
        &arx_file,
        "--root-dir",
        "OrcBlIw/tuyuMO7",
        "-C",
        extract_dir.path()
    );
    assert!(output.status.success());

    let mut source_sub_dir = source_mount_point.to_path_buf();
    source_sub_dir.push("OrcBlIw");
    source_sub_dir.push("tuyuMO7");

    println!(
        "Diff {} and {}",
        source_sub_dir.display(),
        extract_dir.path().display()
    );
    let output = cmd!("diff", "-r", &source_sub_dir, extract_dir.path());
    println!("Out: {}", String::from_utf8(output.stdout).unwrap());
    println!("Err: {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
}

#[cfg(all(unix, not(feature = "in_ci")))]
#[test]
fn test_create_and_extract_subfile() {
    use inner::*;

    let (_source_mount_handle, source_mount_point) = spawn_mount().unwrap();
    let source_mount_point = source_mount_point.path();
    let arx_tmp_dir = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .expect("Creating tempdir should work");
    let arx_file = arx_tmp_dir.path().join("test.arx");
    let output = cmd!(
        "arx",
        "create",
        "--outfile",
        &arx_file,
        "-C",
        source_mount_point.parent().unwrap(),
        "--strip-prefix",
        source_mount_point.file_name().unwrap(),
        source_mount_point.file_name().unwrap()
    );
    println!("Out : {}", String::from_utf8(output.stdout).unwrap());
    println!("Err : {}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
    assert!(arx_file.is_file());

    let extract_dir =
        tempfile::TempDir::with_prefix_in("extract_", env!("CARGO_TARGET_TMPDIR")).unwrap();

    let output = cmd!(
        "arx",
        "extract",
        &arx_file,
        "--root-dir",
        "OrcBlIw/8w5EKLr.text",
        "-C",
        extract_dir.path()
    );
    assert!(!output.status.success());
}
