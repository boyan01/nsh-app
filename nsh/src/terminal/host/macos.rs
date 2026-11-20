//! User-directed recovery for the shared macOS desktop data directory.

use std::{error::Error, io, mem::MaybeUninit};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationPolicy,
    NSModalResponse, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSLocale, NSString, NSURL};

const FULL_DISK_ACCESS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

pub(super) fn should_explain_access(error: &(dyn Error + 'static)) -> bool {
    // The instance lock is checked before opening account data. Filesystem
    // permission failures there arrive as io::Error, separately from logging.
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::PermissionDenied)
        && std::env::var_os("MIXIN_DESKTOP_DATA_DIR").is_none_or(|value| value.is_empty())
}

pub(super) fn explain_access() -> io::Result<()> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("The access dialog must run on the macOS main thread"))?;
    objc2::rc::autoreleasepool(|_| {
        let application = NSApplication::sharedApplication(mtm);
        if !application.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
            return Err(io::Error::other(
                "Open System Settings > Privacy & Security > Full Disk Access from your Mac desktop, allow your terminal host, then quit and reopen it",
            ));
        }
        let chinese = NSLocale::preferredLanguages()
            .firstObject()
            .is_some_and(|language| language.to_string().starts_with("zh"));
        let host = shell_host();
        let alert = access_alert(mtm, chinese, host.as_deref());
        application.activate();
        handle_response(alert.runModal())
    })
}

fn access_alert(mtm: MainThreadMarker, chinese: bool, host: Option<&str>) -> Retained<NSAlert> {
    let (title, details, settings, cancel) = if chinese {
        let host = host.map_or_else(
            || "启动此 shell 的终端或 IDE".to_owned(),
            |name| format!("启动此 shell 的 {name}"),
        );
        (
            "Mixin 需要访问桌面资料",
            format!(
                "macOS 拒绝了对 Mixin 桌面资料目录的访问。请在「系统设置 → 隐私与安全性 → 完全磁盘访问权限」中允许{host}。如果 macOS 的授权提示显示了其他宿主名称，请以系统提示为准。\n\n完全退出并重新打开该宿主，然后再运行 mixin --tui。此对话框是 Mixin 的说明，授权由 macOS 管理。"
            ),
            "打开系统设置",
            "取消",
        )
    } else {
        let host = host.map_or("the terminal or IDE that launched this shell", |name| name);
        (
            "Mixin needs access to desktop data",
            format!(
                "macOS denied access to the Mixin desktop data directory. In System Settings → Privacy & Security → Full Disk Access, allow {host}. If macOS names a different host in its permission prompt, follow the system prompt.\n\nQuit and reopen that host, then run mixin --tui again. This message is from Mixin. macOS manages the permission."
            ),
            "Open System Settings",
            "Cancel",
        )
    };
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(&details));
    alert.addButtonWithTitle(&NSString::from_str(settings));
    let cancel = alert.addButtonWithTitle(&NSString::from_str(cancel));
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    alert
}

fn handle_response(response: NSModalResponse) -> io::Result<()> {
    if response == NSAlertFirstButtonReturn {
        let url = NSURL::URLWithString(&NSString::from_str(FULL_DISK_ACCESS_URL))
            .expect("valid Full Disk Access settings URL");
        if !NSWorkspace::sharedWorkspace().openURL(&url) {
            return Err(io::Error::other(
                "Could not open System Settings. Open Privacy & Security > Full Disk Access manually",
            ));
        }
    }
    Ok(())
}

fn shell_host() -> Option<String> {
    let mut pid = libc::pid_t::try_from(std::process::id()).ok()?;
    // A daemon or detached PTY may have no desktop app in its parent chain.
    // macOS's permission prompt remains authoritative for TCC attribution.
    for _ in 0..32 {
        pid = parent_pid(pid)?;
        if pid <= 1 {
            break;
        }
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            && app.bundleIdentifier().is_some()
            && let Some(name) = app.localizedName()
        {
            return Some(name.to_string());
        }
    }
    None
}

fn parent_pid(pid: libc::pid_t) -> Option<libc::pid_t> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: The output buffer has the size of PROC_PIDTBSDINFO. Only a
    // successful full read initializes the struct.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    info.pbi_ppid.try_into().ok()
}
