pub const TEAM_MIXIN_USER_ID: &str = "773e5e77-4107-45c2-b648-8fc722ed77f5";

/// With PHONE:WRITE the server answers 10006 (update required) to
/// `POST /signal/keys` and `GET /search` after a QR-code login.
pub const SCP: &str = "PROFILE:READ PROFILE:WRITE PHONE:READ CONTACTS:READ CONTACTS:WRITE MESSAGES:READ MESSAGES:WRITE ASSETS:READ ASSETS:ENTRIES:READ SNAPSHOTS:READ COLLECTIBLES:READ CIRCLES:READ CIRCLES:WRITE STICKERS:READ STICKERS:WRITE";

pub const SCP_FULL: &str = "FULL";

#[cfg(test)]
mod tests {
    #[test]
    fn session_scope_reads_entries_and_collectibles_without_phone_write() {
        let scopes: Vec<_> = super::SCP.split(' ').collect();
        assert!(!scopes.contains(&"PHONE:WRITE"));
        for scope in ["PHONE:READ", "ASSETS:ENTRIES:READ", "COLLECTIBLES:READ"] {
            assert!(scopes.contains(&scope), "{scope}");
        }
    }
}
