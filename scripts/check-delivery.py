"""Exercise delivery/release boundaries in disposable directories, never real releases."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run(args, *, cwd=None, env=None, input=None, ok=True):
    result = subprocess.run(args, cwd=cwd, env=env, input=input, text=True, capture_output=True)
    if ok and result.returncode:
        raise AssertionError(f"{args}: {result.stderr}")
    if not ok and not result.returncode:
        raise AssertionError(f"Unexpected success: {args}")
    return result


def package(binary, folder):
    run(["sh", str(ROOT / "scripts/package-release.sh"), str(binary), str(folder)], cwd=ROOT)
    return folder / "ai-session-hub-macos-aarch64.tar.gz"


def check_install(archive, scratch):
    install = scratch / "install"
    config = scratch / "config.toml"
    env = dict(os.environ, AI_SESSION_HUB_CONFIG=str(config))
    args = ["sh", str(ROOT / "scripts/install.sh"), "--archive", str(archive),
            "--install-dir", str(install), "--no-path-update"]
    run(args, env=env)
    binary = install / "bin/ai-session-hub"
    before = binary.read_bytes()
    assert run([str(binary), "--version"]).stdout.startswith("ai-session-hub ")
    config.write_text("# keep me\ntheme = 'ocean'\nrefresh_seconds = 3\n")
    run(args, env=env)
    assert config.read_text() == "# keep me\ntheme = 'ocean'\nrefresh_seconds = 3\n"
    run([str(binary), "setup-config"], env=env)
    run([str(binary), "theme", "forest"], env=env)
    assert "# keep me" in config.read_text() and "refresh_seconds = 3" in config.read_text()
    assert "* forest" in run([str(binary), "themes"], env=env).stdout
    run([str(binary), "theme", "unknown"], env=env, ok=False)
    assert "forest" in config.read_text()
    bad = scratch / "bad.tar.gz"
    bad.write_bytes(archive.read_bytes() + b"corrupted")
    shutil.copyfile(str(archive) + ".sha256", str(bad) + ".sha256")
    run(["sh", str(ROOT / "scripts/install.sh"), "--archive", str(bad),
         "--install-dir", str(install), "--no-path-update"], env=env, ok=False)
    assert binary.read_bytes() == before
    malicious = scratch / "unexpected.tar.gz"
    with tarfile.open(malicious, "w:gz") as tar:
        tar.add(binary, arcname="../ai-session-hub")
    Path(str(malicious) + ".sha256").write_text(hashlib.sha256(malicious.read_bytes()).hexdigest())
    run(["sh", str(ROOT / "scripts/install.sh"), "--archive", str(malicious),
         "--install-dir", str(install), "--no-path-update"], env=env, ok=False)
    assert binary.read_bytes() == before


def check_config_migration(archive, scratch):
    home = scratch / "migration-home"
    config_root = home / "settings"
    legacy = config_root / "fleet-scope/config.toml"
    current = config_root / "ai-session-hub/config.toml"
    legacy.parent.mkdir(parents=True)
    original = b"# user settings\ntheme = 'ocean'\nrefresh_seconds = 3\n"
    legacy.write_bytes(original)
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(config_root))
    env.pop("AI_SESSION_HUB_CONFIG", None)
    args = ["sh", str(ROOT / "scripts/install.sh"), "--archive", str(archive),
            "--install-dir", str(home / "cli"), "--no-path-update"]
    run(args, env=env)
    assert current.read_bytes() == legacy.read_bytes() == original
    sova = config_root / "sova/config.toml"
    sova.parent.mkdir(parents=True)
    sova_settings = b"# Sova settings\ntheme = 'midnight'\nactivity_seconds = 17\n"
    sova.write_bytes(sova_settings)
    current.unlink()
    run(args, env=env)
    assert current.read_bytes() == sova.read_bytes() == sova_settings
    current.write_text("# new settings\ntheme = 'forest'\n")
    run(args, env=env)
    assert current.read_text() == "# new settings\ntheme = 'forest'\n"
    assert legacy.read_bytes() == original
    assert sova.read_bytes() == sova_settings
    override = home / "explicit.toml"
    run(args, env=dict(env, AI_SESSION_HUB_CONFIG=str(override)))
    assert override.read_bytes() == (ROOT / "config.example.toml").read_bytes()
    assert legacy.read_bytes() == original
    assert sova.read_bytes() == sova_settings
    # The installed command and shell PATH agree after a normal installation.
    profile_env = dict(env, SHELL="/bin/zsh")
    run(args[:-1], env=profile_env)
    profile = home / ".zshrc"
    before_profile = profile.read_text()
    run(args[:-1], env=profile_env)
    assert profile.read_text() == before_profile
    resolved = run(["/bin/zsh", "-ic", "command -v ai-session-hub"], env=profile_env).stdout.strip()
    assert resolved == str(home / "cli/bin/ai-session-hub")


def check_release(scratch):
    remote = scratch / "remote.git"
    repo = scratch / "release"
    repo.mkdir()
    env = dict(os.environ, GIT_AUTHOR_NAME="AI Session Hub checks", GIT_AUTHOR_EMAIL="checks@example.invalid",
               GIT_COMMITTER_NAME="AI Session Hub checks", GIT_COMMITTER_EMAIL="checks@example.invalid",
               GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="commit.gpgsign", GIT_CONFIG_VALUE_0="false")
    git = lambda *args, ok=True: run(["git", *args], cwd=repo, env=env, ok=ok)
    run(["git", "init", "--bare", "--initial-branch=main", str(remote)], env=env)
    git("init", "--initial-branch=main")
    (repo / "Cargo.toml").write_text('[package]\nname = "ai-session-hub"\nversion = "0.0.0"\n')
    (repo / "Cargo.lock").write_text('[[package]]\nname = "ai-session-hub"\nversion = "0.0.0"\n')
    (repo / "CHANGELOG.md").write_text("# Changelog\n\n## [Unreleased]\n\n- Test behavior.\n")
    (repo / "Makefile").write_text("check:\n\t@test ! -f fail-check\n")
    shutil.copy(ROOT / "scripts/release.sh", repo / "release.sh")
    git("add", ".")
    git("commit", "-m", "Initial fixture")
    git("remote", "add", "origin", str(remote))
    git("push", "-u", "origin", "main")
    run(["sh", "release.sh"], cwd=repo, env=env, input="bad\n", ok=False)
    assert git("status", "--porcelain").stdout == ""
    (repo / "dirty").write_text("unrelated")
    run(["sh", "release.sh"], cwd=repo, env=env, input="0.1.0\n", ok=False)
    (repo / "dirty").unlink()
    (repo / "fail-check").write_text("")
    git("add", "fail-check")
    git("commit", "-m", "Fail required checks")
    run(["sh", "release.sh"], cwd=repo, env=env, input="0.1.0\n", ok=False)
    assert git("tag").stdout == ""
    git("rm", "fail-check")
    git("commit", "-m", "Restore checks")
    run(["sh", "release.sh"], cwd=repo, env=env, input="0.1.0\n")
    assert git("cat-file", "-t", "v0.1.0").stdout.strip() == "tag"
    assert 'version = "0.1.0"' in (repo / "Cargo.lock").read_text()
    assert "## [0.1.0] - " in (repo / "CHANGELOG.md").read_text()
    assert git("status", "--porcelain").stdout == ""
    run(["sh", "release.sh"], cwd=repo, env=env, input="0.1.0\n", ok=False)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="delivery-check-", dir=target.parent) as folder:
        scratch = Path(folder)
        archive = args.archive or package(target / "release/ai-session-hub", scratch / "dist")
        check_install(archive, scratch)
        check_config_migration(archive, scratch)
        if not args.archive:
            check_release(scratch)
    print("Delivery checks passed: install/config preservation and legacy migration, checksum/archive refusal, isolated release flow")


if __name__ == "__main__":
    main()
