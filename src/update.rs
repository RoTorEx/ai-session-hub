use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;

const APP_NAME: &str = "fleet-scope";
const RELEASE_BASE_URL: &str = "https://github.com/RoTorEx/fleet-scope/releases/latest/download";
const MACOS_AARCH64_ASSET: &str = "fleet-scope-macos-aarch64.tar.gz";
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub fn run() -> Result<()> {
    let asset = release_asset_for(env::consts::OS, env::consts::ARCH).ok_or_else(|| {
        anyhow::anyhow!("fleet-scope update currently supports Apple Silicon macOS only")
    })?;
    let current_exe =
        env::current_exe().context("cannot locate the current fleet-scope executable")?;
    let temp_dir = UpdateTempDir::create()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let archive_url = format!("{RELEASE_BASE_URL}/{asset}");
    let checksum_url = format!("{archive_url}.sha256");
    eprintln!("Downloading {archive_url}");

    let archive_bytes = download_bytes(&client, &archive_url)?;
    let checksum_bytes = download_bytes(&client, &checksum_url)?;
    let checksum_text =
        std::str::from_utf8(&checksum_bytes).context("release checksum is not valid UTF-8")?;
    verify_checksum(&archive_bytes, checksum_text)?;

    let updated = temp_dir.path().join(APP_NAME);
    extract_binary(&archive_bytes, &updated)?;
    set_executable(&updated)?;

    let updated_version = read_version(&updated)?;
    let current_version = format!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
    if updated_version == current_version {
        println!("Already up to date: {updated_version}");
        return Ok(());
    }
    if version_number(&updated_version)? < version_number(&current_version)? {
        bail!("Latest release is older than this binary; refusing downgrade");
    }

    install_binary(&updated, &current_exe)?;
    println!("Updated {current_version} -> {updated_version}");
    Ok(())
}

fn release_asset_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some(MACOS_AARCH64_ASSET),
        _ => None,
    }
}

fn download_bytes(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("release download failed for {url}"))?;

    Ok(response
        .bytes()
        .with_context(|| format!("failed to read {url}"))?
        .to_vec())
}

fn verify_checksum(archive: &[u8], checksum_file: &str) -> Result<()> {
    let expected = parse_checksum(checksum_file)?;
    let actual = format!("{:x}", Sha256::digest(archive));
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("release archive checksum mismatch");
    }
    Ok(())
}

fn parse_checksum(checksum_file: &str) -> Result<&str> {
    let checksum = checksum_file
        .split_whitespace()
        .next()
        .context("release checksum file is empty")?;
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("release checksum is not a SHA-256 digest");
    }
    Ok(checksum)
}

fn extract_binary(archive_bytes: &[u8], destination: &Path) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = Archive::new(decoder);
    let mut found = false;

    for entry in archive.entries().context("cannot read release archive")? {
        let mut entry = entry.context("cannot read release archive entry")?;
        if entry
            .path()
            .context("release archive path is invalid")?
            .as_ref()
            != Path::new(APP_NAME)
        {
            continue;
        }
        if found || !entry.header().entry_type().is_file() {
            bail!("release archive contains an invalid fleet-scope entry");
        }
        entry
            .unpack(destination)
            .context("cannot extract updated fleet-scope binary")?;
        found = true;
    }

    if !found {
        bail!("release archive does not contain a fleet-scope binary");
    }
    Ok(())
}

fn read_version(binary: &Path) -> Result<String> {
    let mut cmd = Command::new(binary);
    cmd.arg("-V");
    let version = crate::command::output(cmd, std::time::Duration::from_secs(5))?;
    let version = version.trim();
    version_number(version)?;
    Ok(version.to_owned())
}

fn version_number(text: &str) -> Result<[u64; 3]> {
    let number = text
        .strip_prefix("fleet-scope ")
        .context("Invalid binary name in version")?;
    let parts = number
        .split('.')
        .map(|p| p.parse::<u64>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    parts
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid release version"))
}

fn install_binary(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("current fleet-scope executable has no parent directory")?;
    let temporary = parent.join(format!(".fleet-scope-update-{}", std::process::id()));

    let result = (|| -> Result<()> {
        fs::copy(source, &temporary).with_context(|| {
            format!("cannot copy updated fleet-scope to {}", temporary.display())
        })?;
        set_executable(&temporary)?;
        fs::rename(&temporary, destination)
            .with_context(|| format!("cannot replace {}", destination.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn set_executable(path: &Path) -> Result<()> {
    let mut permissions = fs::metadata(path)
        .with_context(|| format!("cannot inspect {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("cannot make {} executable", path.display()))
}

struct UpdateTempDir {
    path: PathBuf,
}

impl UpdateTempDir {
    fn create() -> Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time is before the Unix epoch")?
            .as_nanos();
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "fleet-scope-update-{}-{suffix}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .with_context(|| format!("cannot create update directory {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for UpdateTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_update_directories_are_independent_and_cleaned_on_drop() {
        let workers: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..16)
                        .map(|_| UpdateTempDir::create().unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let directories: Vec<_> = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect();
        let paths: std::collections::HashSet<_> = directories
            .iter()
            .map(|dir| dir.path().to_owned())
            .collect();
        assert_eq!(paths.len(), directories.len());
        assert!(paths.iter().all(|path| path.is_dir()));
        drop(directories);
        assert!(paths.iter().all(|path| !path.exists()));
    }

    fn archive(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gzip);
        for (name, kind, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(*kind);
            if kind.is_symlink() {
                header.set_link_name("outside").unwrap();
            }
            header.set_cksum();
            builder.append_data(&mut header, name, *content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn extracts_only_the_exact_binary_without_unpacking_other_paths() {
        let temp = UpdateTempDir::create().unwrap();
        let destination = temp.path().join("new-binary");
        let bytes = archive(&[
            ("nested/fleet-scope", tar::EntryType::Regular, b"wrong"),
            ("config.toml", tar::EntryType::Regular, b"unwanted"),
            (APP_NAME, tar::EntryType::Regular, b"replacement"),
        ]);
        extract_binary(&bytes, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn rejects_missing_duplicate_and_symlink_binaries() {
        let temp = UpdateTempDir::create().unwrap();
        let cases = [
            archive(&[("nested/fleet-scope", tar::EntryType::Regular, b"wrong")]),
            archive(&[
                (APP_NAME, tar::EntryType::Regular, b"first"),
                (APP_NAME, tar::EntryType::Regular, b"second"),
            ]),
            archive(&[(APP_NAME, tar::EntryType::Symlink, b"")]),
        ];
        for (index, bytes) in cases.iter().enumerate() {
            assert!(extract_binary(bytes, &temp.path().join(index.to_string())).is_err());
        }
    }

    #[test]
    fn replacement_preserves_old_binary_on_failure_and_leaves_no_staging_file() {
        let temp = UpdateTempDir::create().unwrap();
        let destination = temp.path().join(APP_NAME);
        let source = temp.path().join("downloaded");
        fs::write(&destination, b"old").unwrap();
        assert!(install_binary(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        fs::write(&source, b"new").unwrap();
        install_binary(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn selects_only_the_published_apple_silicon_asset() {
        assert_eq!(
            release_asset_for("macos", "aarch64"),
            Some("fleet-scope-macos-aarch64.tar.gz")
        );
        assert_eq!(release_asset_for("macos", "x86_64"), None);
        assert_eq!(release_asset_for("linux", "aarch64"), None);
    }

    #[test]
    fn accepts_matching_sha256_checksum() {
        verify_checksum(
            b"fleet-scope release",
            "fd857f92294177a7a510d39f242a16a725db8e7dd2a2f1976b693f1fc59301c3  fleet-scope.tar.gz",
        )
        .unwrap();
    }

    #[test]
    fn rejects_mismatched_sha256_checksum() {
        let error = verify_checksum(
            b"tampered",
            "fd857f92294177a7a510d39f242a16a725db8e7dd2a2f1976b693f1fc59301c3  fleet-scope.tar.gz",
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "release archive checksum mismatch");
    }

    #[test]
    fn rejects_malformed_checksum() {
        let error = parse_checksum("not-a-checksum fleet-scope.tar.gz").unwrap_err();

        assert_eq!(
            error.to_string(),
            "release checksum is not a SHA-256 digest"
        );
    }
}
