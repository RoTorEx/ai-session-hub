use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Codex,
    Claude,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub started: u64,
    pub tty: u32,
    pub stdin_tty: Option<u32>,
    pub cwd: Option<PathBuf>,
    pub provider: Option<Provider>,
    pub launcher: bool,
}

fn classify(executable: &str, args: &[String]) -> Option<Provider> {
    let name = Path::new(executable).file_name()?.to_str()?;
    let argv0 = args
        .first()
        .and_then(|s| Path::new(s).file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let provider = if name == "codex" || argv0 == "codex" {
        Provider::Codex
    } else if name == "claude"
        || argv0 == "claude"
        || executable.contains("/.local/share/claude/versions/")
    {
        Provider::Claude
    } else if name == "node"
        && args.get(1).is_some_and(|s| {
            s.contains("/@anthropic-ai/claude-code/") || s.contains("/@openai/codex/")
        })
    {
        if args[1].contains("claude-code") {
            Provider::Claude
        } else {
            Provider::Codex
        }
    } else {
        return None;
    };
    let offset = if name == "node" { 2 } else { 1 };
    let tail = args.get(offset..).unwrap_or_default();
    // Only launch-mode tokens are inspected; argv/environment never leave discovery.
    let blocked = match provider {
        Provider::Codex => &[
            "exec",
            "e",
            "review",
            "app-server",
            "mcp-server",
            "mcp",
            "login",
            "logout",
            "completion",
            "debug",
            "apply",
            "--help",
            "-h",
            "--version",
            "-V",
        ][..],
        Provider::Claude => &[
            "--print",
            "-p",
            "--help",
            "-h",
            "--version",
            "-v",
            "mcp",
            "update",
            "install",
            "doctor",
            "auth",
            "agents",
            "gateway",
            "plugin",
            "plugins",
            "purge",
            "respawn",
            "rm",
            "logs",
            "stop",
            "kill",
            "ultrareview",
            "setup-token",
            "import",
            "upgrade",
            "auto-mode",
            "--bg",
            "--background",
            "--desktop",
            "--cloud",
        ][..],
    };
    let mut i = 0;
    let mut first_positional = true;
    while i < tail.len() {
        let arg = &tail[i];
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') {
            if blocked.contains(&arg.as_str())
                || (provider == Provider::Claude
                    && (arg.starts_with("--print=") || (arg.starts_with("-p") && arg.len() > 2)))
            {
                return None;
            }
            let takes_value = match provider {
                Provider::Codex => &[
                    "-m",
                    "--model",
                    "-c",
                    "--config",
                    "-C",
                    "--cd",
                    "-a",
                    "--ask-for-approval",
                    "-s",
                    "--sandbox",
                    "-p",
                    "--profile",
                    "-i",
                    "--image",
                    "--add-dir",
                ][..],
                Provider::Claude => &[
                    "-m",
                    "--model",
                    "--agent",
                    "--agents",
                    "--debug-file",
                    "--effort",
                    "--input-format",
                    "--output-format",
                    "--max-budget-usd",
                    "--fallback-model",
                    "--mcp-config",
                    "--plugin-dir",
                    "--json-schema",
                    "--setting-sources",
                    "--permission-mode",
                    "--system-prompt",
                    "--append-system-prompt",
                    "--settings",
                    "--allowedTools",
                    "--disallowedTools",
                ][..],
            };
            if takes_value.contains(&arg.as_str()) {
                i += 1;
            }
        } else {
            if first_positional && blocked.contains(&arg.as_str()) {
                return None;
            }
            first_positional = false;
        }
        i += 1;
    }
    Some(provider)
}

pub fn agents(table: &[Process]) -> Vec<Process> {
    let by_pid: HashMap<_, _> = table.iter().map(|p| (p.pid, p)).collect();
    table
        .iter()
        .filter(|p| {
            if p.provider.is_none() || p.tty == u32::MAX || p.stdin_tty != Some(p.tty) {
                return false;
            }
            if p.launcher
                && table.iter().any(|child| {
                    child.pid != p.pid
                        && !child.launcher
                        && child.provider == p.provider
                        && child.tty == p.tty
                        && descendant(child.pid, p.pid, table)
                })
            {
                return false;
            }
            let mut ancestor = p.parent;
            let mut visited = HashSet::new();
            while let Some(parent) = by_pid.get(&ancestor) {
                if !visited.insert(ancestor) {
                    return false;
                }
                if parent.provider == p.provider && parent.tty == p.tty && !parent.launcher {
                    return false;
                }
                ancestor = parent.parent;
            }
            true
        })
        .cloned()
        .collect()
}

pub fn descendant(pid: u32, ancestor: u32, table: &[Process]) -> bool {
    let mut cursor = pid;
    let mut visited = HashSet::new();
    while cursor != 0 && visited.insert(cursor) {
        if cursor == ancestor {
            return true;
        }
        let Some(p) = table.iter().find(|p| p.pid == cursor) else {
            return false;
        };
        cursor = p.parent;
    }
    false
}

#[cfg(target_os = "macos")]
pub fn scan() -> Result<Vec<Process>> {
    use std::mem::{size_of, zeroed};
    fn read_info<T>(pid: i32, flavor: i32) -> Option<T> {
        // The libproc structures are C POD, zero is valid, and size is exact.
        let mut info: T = unsafe { zeroed() };
        let count = unsafe {
            libc::proc_pidinfo(
                pid,
                flavor,
                0,
                (&mut info as *mut T).cast(),
                size_of::<T>() as i32,
            )
        };
        (count == size_of::<T>() as i32).then_some(info)
    }
    fn c_text(bytes: &[libc::c_char]) -> String {
        let data: Vec<_> = bytes
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&data).into_owned()
    }
    fn argv(pid: i32) -> Option<Vec<String>> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut len = 0;
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                std::ptr::null_mut(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
            || len < 4
        {
            return None;
        }
        let mut bytes = vec![0u8; len];
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                bytes.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        bytes.truncate(len);
        let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        if !(1..=4096).contains(&argc) {
            return None;
        }
        let mut pos = 4;
        while *bytes.get(pos)? != 0 {
            pos += 1;
        }
        while bytes.get(pos) == Some(&0) {
            pos += 1;
        }
        let mut args = Vec::new();
        for _ in 0..argc {
            let end = bytes.get(pos..)?.iter().position(|&b| b == 0)? + pos;
            args.push(String::from_utf8_lossy(&bytes[pos..end]).into_owned());
            pos = end + 1;
        }
        Some(args)
    }
    #[repr(C)]
    struct FileInfo {
        flags: u32,
        status: u32,
        offset: i64,
        kind: i32,
        reserved: i32,
    }
    #[repr(C)]
    struct FdPath {
        file: FileInfo,
        vnode: libc::vnode_info_path,
    }
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        bail!(
            "Cannot enumerate processes: {}",
            std::io::Error::last_os_error()
        );
    }
    let mut pids = vec![0i32; count as usize + 256];
    let n = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * size_of::<i32>()) as i32,
        )
    };
    if n <= 0 {
        bail!("Cannot read process snapshot");
    }
    let uid = unsafe { libc::getuid() };
    let mut table = Vec::new();
    for pid in pids.into_iter().take(n as usize) {
        let Some(info) = read_info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO) else {
            continue;
        };
        if info.pbi_status == 5 {
            continue;
        }
        let mut p = Process {
            pid: pid as u32,
            parent: info.pbi_ppid,
            started: info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
            tty: info.e_tdev,
            stdin_tty: None,
            cwd: None,
            provider: None,
            launcher: false,
        };
        if p.tty != u32::MAX && info.pbi_uid == uid {
            let mut path = [0i8; 4096];
            let len =
                unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
            if len > 0
                && let Some(args) = argv(pid)
            {
                p.provider = classify(&c_text(&path), &args);
                p.launcher = Path::new(&c_text(&path))
                    .file_name()
                    .is_some_and(|s| s == "node");
            }
            if p.provider.is_some() {
                let mut fd: FdPath = unsafe { zeroed() };
                // PROC_PIDFDVNODEPATHINFO = 2, from the macOS SDK proc_info.h.
                let n = unsafe {
                    libc::proc_pidfdinfo(
                        pid,
                        0,
                        2,
                        (&mut fd as *mut FdPath).cast(),
                        size_of::<FdPath>() as i32,
                    )
                };
                if n == size_of::<FdPath>() as i32 {
                    let stdin_path = c_text(&fd.vnode.vip_path.concat());
                    use std::os::unix::fs::MetadataExt;
                    p.stdin_tty = std::fs::metadata(stdin_path).ok().map(|m| m.rdev() as u32);
                }
                if let Some(cwd) =
                    read_info::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO)
                {
                    // Kernel provides a NUL-terminated MAXPATHLEN buffer.
                    let path = c_text(&cwd.pvi_cdir.vip_path.concat());
                    if !path.is_empty() {
                        p.cwd = Some(PathBuf::from(path));
                    }
                }
            }
        }
        table.push(p);
    }
    Ok(table)
}

#[cfg(not(target_os = "macos"))]
pub fn scan() -> Result<Vec<Process>> {
    bail!("AI Session Hub supports macOS only")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn agent(pid: u32, parent: u32) -> Process {
        Process {
            pid,
            parent,
            started: 1,
            tty: 7,
            stdin_tty: Some(7),
            cwd: None,
            provider: Some(Provider::Codex),
            launcher: false,
        }
    }
    #[test]
    fn keeps_independent_instances_but_excludes_helpers_and_piped_commands() {
        let mut piped = agent(4, 0);
        piped.stdin_tty = None;
        assert_eq!(
            agents(&[agent(1, 0), agent(2, 1), agent(3, 0), piped])
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }
    #[test]
    fn recognizes_interactive_resume_and_excludes_noninteractive_modes() {
        let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            classify("/bin/codex", &args("codex resume")),
            Some(Provider::Codex)
        );
        assert_eq!(classify("/bin/codex", &args("codex exec")), None);
        assert_eq!(classify("/bin/claude", &args("claude --print hi")), None);
        assert_eq!(
            classify("/bin/claude", &args("claude")),
            Some(Provider::Claude)
        );
        assert_eq!(
            classify("/bin/codex", &args("codex --model review resume")),
            Some(Provider::Codex)
        );
        assert_eq!(
            classify("/bin/codex", &args("codex --model model exec")),
            None
        );
        assert_eq!(classify("/bin/claude", &args("claude --print=hello")), None);
        assert_eq!(
            classify("/bin/claude", &args("claude -c --print hello")),
            None
        );
        assert_eq!(
            classify(
                "/bin/claude",
                &args("claude --output-format json --print hello")
            ),
            None
        );
        assert_eq!(classify("/bin/claude", &args("claude -phello")), None);
        assert_eq!(classify("/bin/claude", &args("claude --background")), None);
    }
    #[test]
    fn ancestry_crosses_unclassified_login_but_never_an_unrelated_root() {
        let mut login = agent(1, 0);
        login.provider = None;
        let mut shell = agent(2, 1);
        shell.provider = None;
        let table = [login, shell, agent(3, 2)];
        assert!(descendant(3, 1, &table));
        assert!(!descendant(3, 9, &table));
        assert_eq!(agents(&table).len(), 1);
    }
    #[test]
    fn native_child_replaces_node_launcher_without_duplicate_or_wrong_cwd() {
        let mut launcher = agent(1, 0);
        launcher.launcher = true;
        launcher.cwd = Some("/launcher".into());
        let mut native = agent(2, 1);
        native.cwd = Some("/agent".into());
        let found = agents(&[launcher, native]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].pid, 2);
        assert_eq!(found[0].cwd.as_deref(), Some(Path::new("/agent")));
    }
}
