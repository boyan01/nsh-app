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
        // WMIC is gone from recent Windows releases. SMBIOS holds the UUID it
        // reported, so accounts bound to that id keep it.
        return windows_smbios_uuid()
            .or_else(|| windows_uuid_command("wmic", &["csproduct", "get", "UUID"]))
            .or_else(|| {
                windows_uuid_command(
                    "powershell.exe",
                    &[
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "(Get-CimInstance -ClassName Win32_ComputerSystemProduct).UUID",
                    ],
                )
            });
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

#[cfg(target_os = "windows")]
fn windows_smbios_uuid() -> Option<String> {
    // 'RSMB', the raw SMBIOS firmware table provider.
    const RSMB: u32 = 0x5253_4D42;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemFirmwareTable(provider: u32, id: u32, buffer: *mut u8, size: u32) -> u32;
    }
    // SAFETY: a null buffer with size 0 asks only for the table size.
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, std::ptr::null_mut(), 0) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0_u8; size as usize];
    // SAFETY: `buffer` holds `size` writable bytes.
    let written = unsafe { GetSystemFirmwareTable(RSMB, 0, buffer.as_mut_ptr(), size) };
    if written == 0 || written > size {
        return None;
    }
    buffer.truncate(written as usize);
    smbios_system_uuid(&buffer)
}

#[cfg(target_os = "windows")]
fn windows_uuid_command(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.eq_ignore_ascii_case("uuid"))
        .map(str::to_lowercase)
}

/// The UUID of the System Information (type 1) structure in the
/// `RawSMBIOSData` that `GetSystemFirmwareTable('RSMB')` returns, formatted
/// as WMI reports it (first three fields little-endian), lowercased.
#[cfg(any(target_os = "windows", test))]
fn smbios_system_uuid(raw: &[u8]) -> Option<String> {
    // RawSMBIOSData: 4 bytes of version info, a 4-byte length, then the table.
    let length = u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?) as usize;
    let table = raw.get(8..)?;
    let table = &table[..length.min(table.len())];
    let mut offset = 0;
    while offset + 4 <= table.len() {
        let kind = table[offset];
        let formatted = table[offset + 1] as usize;
        if formatted < 4 || offset + formatted > table.len() {
            return None;
        }
        match kind {
            1 => {
                // The UUID occupies offsets 08h to 17h.
                if formatted < 0x18 {
                    return None;
                }
                let bytes = &table[offset + 8..offset + 0x18];
                return Some(uuid::Uuid::from_bytes_le(bytes.try_into().ok()?).to_string());
            }
            127 => return None,
            _ => {}
        }
        // Skip the formatted area, then the string set ending in two zeros.
        let strings = &table[offset + formatted..];
        offset += formatted + strings.windows(2).position(|pair| pair == [0, 0])? + 2;
    }
    None
}

#[cfg(test)]
mod tests {
    fn raw(table: &[&[u8]]) -> Vec<u8> {
        let table = table.concat();
        let mut raw = vec![0, 3, 4, 0];
        raw.extend((table.len() as u32).to_le_bytes());
        raw.extend(table);
        raw
    }

    // BIOS Information (type 0) with the strings "Vendor" and "1.0".
    const BIOS: &[u8] = &[
        0, 0x18, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'V', b'e',
        b'n', b'd', b'o', b'r', 0, b'1', b'.', b'0', 0, 0,
    ];
    const END: &[u8] = &[127, 4, 0xff, 0xff, 0, 0];

    fn system(uuid: [u8; 16]) -> Vec<u8> {
        let mut system = vec![1, 0x1b, 1, 0, 1, 2, 3, 4];
        system.extend(uuid);
        system.extend([6, 0, 0, 0, 0]);
        system
    }

    #[test]
    fn smbios_system_uuid_matches_wmi_and_rejects_tables_without_one() {
        let uuid: [u8; 16] = std::array::from_fn(|i| i as u8 * 0x11);
        assert_eq!(
            super::smbios_system_uuid(&raw(&[BIOS, &system(uuid), END])).as_deref(),
            Some("33221100-5544-7766-8899-aabbccddeeff"),
        );
        assert_eq!(super::smbios_system_uuid(&raw(&[BIOS, END])), None);
        let truncated = system([0xab; 16]);
        assert_eq!(
            super::smbios_system_uuid(&raw(&[BIOS, &truncated[..10]])),
            None
        );
    }
}
