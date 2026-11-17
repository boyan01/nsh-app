#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn current_device_id() -> Option<String> {
    tokio::task::spawn_blocking(current_device_id_sync)
        .await
        .ok()
        .flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn current_device_id_sync() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let id = std::fs::read_to_string("/etc/machine-id").ok()?;
        let id = id.trim();
        if id.len() != 32 {
            return None;
        }
        return Some(name_uuid_from_bytes(id.as_bytes()));
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;
        let output = String::from_utf8(output.stdout).ok()?;
        return output.lines().find_map(|line| {
            if !line.contains("IOPlatformUUID") {
                return None;
            }
            line.split('=')
                .nth(1)
                .map(|value| value.trim().trim_matches('"').to_lowercase())
                .filter(|value| !value.is_empty())
        });
    }
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("wmic")
            .args(["csproduct", "get", "UUID"])
            .output()
            .ok()?;
        let output = String::from_utf8(output.stdout).ok()?;
        return output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.eq_ignore_ascii_case("uuid"))
            .map(str::to_lowercase);
    }
    #[allow(unreachable_code)]
    None
}

/// Java's `UUID.nameUUIDFromBytes`, which the Flutter app applies to the
/// Linux machine id.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn name_uuid_from_bytes(bytes: &[u8]) -> String {
    let mut digest = md5::compute(bytes).0;
    digest[6] = (digest[6] & 0x0f) | 0x30;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(digest).to_string()
}
