use std::process::Command;

use patchouli_provider::Provider;
use patchouli_provider_sqlite::SqliteProvider;
use serde_json::Value;

#[test]
fn init_creates_valid_configuration_without_overwriting_existing_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("patchouli-home");
    let binary = env!("CARGO_BIN_EXE_patchouli-db");

    let first = Command::new(binary)
        .args(["init", "--root"])
        .arg(&root)
        .output()
        .expect("run init");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(root.join("data").is_dir());
    assert!(root.join("run").is_dir());
    assert!(root.join("patchouli.schema.json").is_file());
    assert!(root.join("providers.schema.json").is_file());

    let policy: Value =
        serde_json::from_slice(&std::fs::read(root.join("config.json")).expect("read policy"))
            .expect("valid policy JSON");
    let providers: Value = serde_json::from_slice(
        &std::fs::read(root.join("providers.json")).expect("read providers"),
    )
    .expect("valid provider JSON");
    assert_eq!(policy["version"], 1);
    assert_eq!(
        providers["providers"]["local"]["database"],
        "data/patchouli.db"
    );

    let repeated = Command::new(binary)
        .args(["init", "--root"])
        .arg(&root)
        .output()
        .expect("rerun valid init");
    assert!(repeated.status.success());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for directory in [&root, &root.join("data"), &root.join("run")] {
            let mode = std::fs::metadata(directory)
                .expect("directory metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
        for file in [
            root.join("config.json"),
            root.join("providers.json"),
            root.join("patchouli.schema.json"),
            root.join("providers.schema.json"),
        ] {
            let mode = std::fs::metadata(file)
                .expect("configuration metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    let custom = b"{\"custom\":true}\n";
    std::fs::write(root.join("config.json"), custom).expect("replace test policy");
    let second = Command::new(binary)
        .args(["init", "--root"])
        .arg(&root)
        .output()
        .expect("rerun init");
    assert!(
        !second.status.success(),
        "invalid existing policy must be reported"
    );
    assert_eq!(std::fs::read(root.join("config.json")).unwrap(), custom);
}

#[cfg(unix)]
#[test]
fn init_rejects_an_existing_home_visible_to_other_users() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("patchouli-home");
    std::fs::create_dir(&root).expect("create existing home");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))
        .expect("make home non-private");

    let output = Command::new(env!("CARGO_BIN_EXE_patchouli-db"))
        .args(["init", "--root"])
        .arg(&root)
        .output()
        .expect("run init");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("accessible by other users"));
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_an_existing_database_visible_to_other_users() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("patchouli-home");
    let binary = env!("CARGO_BIN_EXE_patchouli-db");
    assert!(
        Command::new(binary)
            .args(["init", "--root"])
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let database = root.join("data/patchouli.db");
    std::fs::write(&database, []).unwrap();
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o644)).unwrap();

    let output = Command::new(binary)
        .args(["init", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("accessible by other users"));
    assert_eq!(
        std::fs::metadata(database).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[tokio::test]
async fn provider_bind_failure_still_records_a_clean_shutdown() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let database = directory.path().join("patchouli.db");
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy address");
    let address = occupied.local_addr().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_patchouli-db"))
        .args(["provide", "--listen", &address.to_string(), "--database"])
        .arg(&database)
        .args(["--token-env", "PATCHOULI_TEST_PROVIDER_TOKEN"])
        .env("PATCHOULI_TEST_PROVIDER_TOKEN", "secret")
        .output()
        .expect("run provider");
    assert!(
        !output.status.success(),
        "occupied address must reject bind"
    );
    drop(occupied);

    let provider = SqliteProvider::open(&database)
        .await
        .expect("reopen database after failed service start");
    let recovery = provider.initialize().await.expect("read recovery state");
    assert!(
        !recovery.recovered_after_unclean_shutdown,
        "failed service startup must shut down the initialized provider"
    );
    provider.shutdown().await.expect("shutdown provider");
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_uses_the_clean_daemon_shutdown_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("home");
    let binary = env!("CARGO_BIN_EXE_patchouli-db");
    assert!(
        Command::new(binary)
            .args(["init", "--root"])
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let endpoint = root.join("run/patchouli.sock");
    let mut child = Command::new(binary)
        .args(["serve", "--endpoint"])
        .arg(&endpoint)
        .arg("--artifacts")
        .arg(root.join("data/artifacts"))
        .arg("--providers")
        .arg(root.join("providers.json"))
        .arg("--config")
        .arg(root.join("config.json"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start daemon");

    for _ in 0..100 {
        if endpoint.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    if !endpoint.exists() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("daemon did not create its endpoint");
    }
    let signal = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("send SIGTERM");
    assert!(signal.success());
    assert!(wait_for_child(&mut child).await.success());

    let provider = SqliteProvider::open(root.join("data/patchouli.db"))
        .await
        .expect("reopen database after SIGTERM");
    let recovery = provider.initialize().await.expect("read recovery state");
    assert!(!recovery.recovered_after_unclean_shutdown);
    provider.shutdown().await.expect("shutdown provider");
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_releases_an_active_remote_change_wait() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let database = directory.path().join("patchouli.db");
    let stderr_path = directory.path().join("provider.stderr");
    let stderr = std::fs::File::create(&stderr_path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_patchouli-db"))
        .args(["provide", "--listen", "127.0.0.1:0", "--database"])
        .arg(&database)
        .args(["--token-env", "PATCHOULI_TEST_PROVIDER_TOKEN"])
        .env("PATCHOULI_TEST_PROVIDER_TOKEN", "secret")
        .stdout(std::process::Stdio::null())
        .stderr(stderr)
        .spawn()
        .expect("start remote provider");
    let prefix = "Patchouli remote provider listening on ";
    let mut address: Option<std::net::SocketAddr> = None;
    for _ in 0..250 {
        let output = std::fs::read_to_string(&stderr_path).unwrap();
        if let Some(value) = output
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .find_map(|value| value.parse().ok())
        {
            address = Some(value);
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("remote provider exited before binding ({status}): {output}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let address = address.unwrap_or_else(|| {
        let output = std::fs::read_to_string(&stderr_path).unwrap();
        let _ = child.kill();
        let _ = child.wait();
        panic!("remote provider did not report its bound address: {output}");
    });
    let client = reqwest::Client::new();
    let info = format!("http://{address}/provider/v2/info");
    let mut ready = false;
    for _ in 0..100 {
        if client
            .get(&info)
            .bearer_auth("secret")
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        let output = std::fs::read_to_string(&stderr_path).unwrap();
        panic!("remote provider did not become ready: {output}");
    }

    let wait = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .post(format!("http://{address}/provider/v2/call"))
                .bearer_auth("secret")
                .json(&serde_json::json!({
                    "method": "wait_for_changes",
                    "params": {"scope_json": "{}", "after_cursor": 0}
                }))
                .send()
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(wait_for_child(&mut child).await.success());
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), wait)
        .await
        .expect("remote wait must finish")
        .unwrap()
        .expect("remote wait response");
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

    let provider = SqliteProvider::open(&database).await.unwrap();
    let recovery = provider.initialize().await.unwrap();
    assert!(!recovery.recovered_after_unclean_shutdown);
    provider.shutdown().await.unwrap();
}

#[cfg(unix)]
async fn wait_for_child(child: &mut std::process::Child) -> std::process::ExitStatus {
    for _ in 0..250 {
        if let Some(status) = child.try_wait().expect("poll child process") {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("process did not stop after SIGTERM");
}

#[cfg(unix)]
#[test]
fn unix_installer_verifies_and_stages_an_upgrade_before_replacement() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary directory");
    let assets = directory.path().join("assets");
    let fake_bin = directory.path().join("fake-bin");
    let install_dir = directory.path().join("install");
    let home = directory.path().join("home");
    std::fs::create_dir(&assets).unwrap();
    std::fs::create_dir(&fake_bin).unwrap();

    let asset_name = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "patchouli-db-linux-x86_64",
        ("linux", "aarch64") => "patchouli-db-linux-aarch64",
        ("macos", "x86_64") => "patchouli-db-macos-x86_64",
        ("macos", "aarch64") => "patchouli-db-macos-aarch64",
        platform => panic!("unsupported installer test platform: {platform:?}"),
    };
    let asset = assets.join(asset_name);
    std::fs::copy(env!("CARGO_BIN_EXE_patchouli-db"), &asset).unwrap();
    write_checksum(&asset, asset_name);

    let fake_curl = fake_bin.join("curl");
    std::fs::write(
        &fake_curl,
        b"#!/bin/sh\nset -eu\noutput=\nurl=\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    -o) shift; output=$1 ;;\n    http*) url=$1 ;;\n  esac\n  shift\ndone\ncp \"$PATCHOULI_TEST_ASSET_DIR/$(basename \"$url\")\" \"$output\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake_curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let installer =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/install.sh");

    let blocked_install = directory.path().join("blocked-install");
    let blocked_target = blocked_install.join("patchouli-db");
    std::fs::create_dir_all(&blocked_target).unwrap();
    let blocked = Command::new("sh")
        .arg(&installer)
        .env("PATH", &path)
        .env("PATCHOULI_TEST_ASSET_DIR", &assets)
        .env("PATCHOULI_INSTALL_DIR", &blocked_install)
        .env("PATCHOULI_HOME", &home)
        .output()
        .expect("run installer against a directory target");
    assert!(!blocked.status.success());
    assert!(blocked_target.is_dir());
    assert_eq!(std::fs::read_dir(blocked_target).unwrap().count(), 0);

    let first = Command::new("sh")
        .arg(&installer)
        .env("PATH", &path)
        .env("PATCHOULI_TEST_ASSET_DIR", &assets)
        .env("PATCHOULI_INSTALL_DIR", &install_dir)
        .env("PATCHOULI_HOME", &home)
        .output()
        .expect("run installer");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let installed = install_dir.join("patchouli-db");
    assert!(
        Command::new(&installed)
            .arg("--version")
            .output()
            .unwrap()
            .status
            .success()
    );

    std::fs::write(&asset, b"#!/bin/sh\nexit 42\n").unwrap();
    std::fs::set_permissions(&asset, std::fs::Permissions::from_mode(0o755)).unwrap();
    write_checksum(&asset, asset_name);
    let failed_upgrade = Command::new("sh")
        .arg(installer)
        .env("PATH", path)
        .env("PATCHOULI_TEST_ASSET_DIR", assets)
        .env("PATCHOULI_INSTALL_DIR", install_dir)
        .env("PATCHOULI_HOME", home)
        .output()
        .expect("run failing upgrade");
    assert!(!failed_upgrade.status.success());
    assert!(
        Command::new(installed)
            .arg("--version")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[cfg(unix)]
fn write_checksum(asset: &std::path::Path, asset_name: &str) {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(asset)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(asset).output())
        .expect("SHA-256 utility");
    assert!(output.status.success());
    let hash = String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    std::fs::write(
        asset.with_file_name(format!("{asset_name}.sha256")),
        format!("{hash}  {asset_name}\n"),
    )
    .unwrap();
}
