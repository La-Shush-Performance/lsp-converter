//! Process helpers - ported from ACECM's _proc_win.py: is the game running,
//! and stopping a conversion. In-process Win32 only (ntdll, Toolhelp); never
//! powershell.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    #[derive(Clone)]
    pub struct Proc {
        pub pid: u32,
        pub name: String,
    }

    fn wstr(buf: &[u16]) -> String {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    static SNAP: std::sync::Mutex<Option<(std::time::Instant, Vec<Proc>)>> = std::sync::Mutex::new(None);
    const SNAP_TTL_MS: u128 = 1500;

    /// Every process, from a snapshot at most 1.5 s old.
    /// ⚠ A snapshot costs ~7 ms (hundreds of processes and their threads),
    /// and a page can ask several times a second. Anything WE spawn or kill
    /// clears it (invalidate_snapshot), so our own changes are seen at once;
    /// a process someone else starts or ends shows up within 1.5 s, well
    /// inside what every waiting loop tolerates.
    pub fn snapshot() -> Vec<Proc> {
        let mut g = SNAP.lock().unwrap();
        if let Some((at, v)) = g.as_ref() {
            if at.elapsed().as_millis() < SNAP_TTL_MS {
                return v.clone();
            }
        }
        let v = snapshot_fresh();
        *g = Some((std::time::Instant::now(), v.clone()));
        v
    }

    pub fn invalidate_snapshot() {
        *SNAP.lock().unwrap() = None;
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQuerySystemInformation(class: u32, buf: *mut core::ffi::c_void, len: u32, ret: *mut u32) -> i32;
    }

    /// Every process, from one NtQuerySystemInformation(SystemProcessInformation)
    /// call - what Toolhelp wraps, at about half its cost (7 vs 12 ms here).
    /// Falls back to Toolhelp if the call or the layout ever disagrees.
    pub fn snapshot_fresh() -> Vec<Proc> {
        // the offsets below are the x64 layout
        if cfg!(not(target_pointer_width = "64")) {
            return toolhelp();
        }
        const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;
        let mut buf: Vec<u64> = vec![0; (2 << 20) / 8];
        for _ in 0..4 {
            let mut need = 0u32;
            let st = unsafe { NtQuerySystemInformation(5, buf.as_mut_ptr() as *mut _, (buf.len() * 8) as u32, &mut need) };
            if st == STATUS_INFO_LENGTH_MISMATCH {
                buf = vec![0; (need as usize + (1 << 16)) / 8 + 1];
                continue;
            }
            if st < 0 {
                break;
            }
            // SYSTEM_PROCESS_INFORMATION (x64): NextEntryOffset @0,
            // ImageName {u16 len, u16 max, ptr} @0x38, UniqueProcessId @0x50
            let base = buf.as_ptr() as *const u8;
            let total = buf.len() * 8;
            let mut out = Vec::new();
            let mut off = 0usize;
            loop {
                if off + 0x60 > total {
                    return toolhelp();
                }
                unsafe {
                    let e = base.add(off);
                    let next = (e as *const u32).read_unaligned() as usize;
                    let nlen = (e.add(0x38) as *const u16).read_unaligned() as usize / 2;
                    let nptr = (e.add(0x40) as *const *const u16).read_unaligned();
                    let pid = (e.add(0x50) as *const usize).read_unaligned() as u32;
                    let name = if nptr.is_null() || nlen == 0 {
                        String::new()
                    } else {
                        let start = nptr as usize;
                        if start < base as usize || start + nlen * 2 > base as usize + total {
                            return toolhelp();
                        }
                        String::from_utf16_lossy(std::slice::from_raw_parts(nptr, nlen))
                    };
                    out.push(Proc { pid, name });
                    if next == 0 {
                        break;
                    }
                    off += next;
                }
            }
            return out;
        }
        toolhelp()
    }

    /// Every process, from one Toolhelp snapshot.
    pub fn toolhelp() -> Vec<Proc> {
        let mut out = vec![];
        unsafe {
            let snap: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap.is_null() {
                return out;
            }
            let mut pe: PROCESSENTRY32W = std::mem::zeroed();
            pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut pe) != 0 {
                loop {
                    out.push(Proc { pid: pe.th32ProcessID, name: wstr(&pe.szExeFile) });
                    if Process32NextW(snap, &mut pe) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        out
    }

    fn open(pid: u32) -> Option<HANDLE> {
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h.is_null() { None } else { Some(h) }
    }

    /// ⚠ Exit code, not "can I open it": a dead process stays openable while
    /// anyone holds a handle to it (WebView2's helpers did, for the shell).
    pub fn alive(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        let Some(h) = open(pid) else { return false };
        let mut code = 0u32;
        let ok = unsafe { GetExitCodeProcess(h, &mut code) } != 0;
        unsafe { CloseHandle(h) };
        ok && code == 259 // STILL_ACTIVE
    }

    pub fn hide(cmd: &mut std::process::Command) {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

#[cfg(windows)]
pub use imp::*;

fn norm(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".exe") { n } else { n + ".exe" }
}

/// PIDs whose exe name matches any of `names` (with or without .exe).
pub fn pids_named(names: &[&str]) -> Vec<u32> {
    let want: Vec<String> = names.iter().map(|n| norm(n)).collect();
    snapshot().into_iter().filter(|p| want.contains(&p.name.to_ascii_lowercase()))
        .map(|p| p.pid).collect()
}

/// Force-kill a process tree. True if it is gone afterwards.
/// ⚠ Report the OUTCOME: taskkill can fail (a process started elevated)
/// while the process lives on.
pub fn kill(pid: u32) -> bool {
    invalidate_snapshot();
    let mut c = std::process::Command::new("taskkill");
    c.args(["/PID", &pid.to_string(), "/T", "/F"]);
    hide(&mut c);
    let ok = c.output().map(|o| o.status.success()).unwrap_or(false);
    invalidate_snapshot();
    if !alive(pid) {
        return true;
    }
    if ok {
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !alive(pid) {
                return true;
            }
        }
    }
    !alive(pid)
}

/// Is the game itself running?
pub fn game_running() -> bool {
    !pids_named(&["AssettoCorsaEVO"]).is_empty()
}

/// A link the app may hand to Windows: a web page (https) or an email
/// address (mailto), nothing else - never a file, a program or this PC's
/// own pages.
pub fn link_ok(url: &str) -> bool {
    let u = url.trim();
    let rest = if let Some(r) = u.strip_prefix("https://") { r } else if let Some(r) = u.strip_prefix("mailto:") { r } else { return false };
    !rest.is_empty() && u.len() <= 2048 && !u.chars().any(|c| c.is_control() || c == ' ' || c == '"')
}

/// Open a URL with the system's default handler (the browser for http, the
/// mail app for mailto). ⚠ The window is WebView2: a link clicked inside it
/// would load in the app's own window, or nowhere, so the page asks for this.
pub fn open_url(url: &str) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (op, u) = (w("open"), w(url));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), u.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        r as isize > 32
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn().is_ok()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn only_web_and_mail_links() {
        assert!(link_ok("https://lashushperformance.com"));
        assert!(link_ok("mailto:contact@lashushperformance.com"));
        for bad in ["http://127.0.0.1:8765/", "file:///C:/Windows/notepad.exe", "C:\\Windows\\notepad.exe",
            "javascript:alert(1)", "https://", "https://a b", "https://x\"y", "ms-settings:"] {
            assert!(!link_ok(bad), "{bad}");
        }
    }

    #[test]
    fn sees_itself() {
        let me = std::process::id();
        assert!(alive(me));
        assert!(snapshot().iter().any(|p| p.pid == me));
    }

    #[test]
    fn ntquery_matches_toolhelp() {
        let a = snapshot_fresh();
        let b = toolhelp();
        let me = std::process::id();
        let mine = a.iter().find(|p| p.pid == me).expect("self in the snapshot");
        let th = b.iter().find(|p| p.pid == me).unwrap();
        assert_eq!(mine.name.to_lowercase(), th.name.to_lowercase());
        // the two calls are a moment apart: allow a little churn
        let pa: std::collections::HashSet<u32> = a.iter().map(|p| p.pid).collect();
        let common = b.iter().filter(|p| pa.contains(&p.pid)).count();
        assert!(common * 100 >= b.len() * 95, "{common} of {}", b.len());
        let same_name = b.iter().filter(|p| p.pid > 4)
            .filter(|p| a.iter().any(|q| q.pid == p.pid && q.name.eq_ignore_ascii_case(&p.name))).count();
        assert!(same_name * 100 >= (b.len() - 2) * 95);
    }

    #[test]
    fn kills_and_reports_dead() {
        let mut c = std::process::Command::new("ping");
        c.args(["-n", "30", "127.0.0.1"]);
        hide(&mut c);
        let mut child = c.spawn().unwrap();
        let pid = child.id();
        // the contract: whoever spawns and then looks itself up clears the cache
        invalidate_snapshot();
        assert!(alive(pid));
        assert!(pids_named(&["PING"]).contains(&pid));
        assert!(kill(pid));
        let _ = child.wait();
        assert!(!alive(pid));
        assert!(!alive(0));
    }
}
