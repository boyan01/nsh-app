//! Messages that arrived marked silent, until the notification batch skips them.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// shortcut: kept in memory and bounded, so a silent message that arrives just
// before a restart can still notify; persist the flag if that matters.
const LIMIT: usize = 512;

#[derive(Clone, Default)]
pub(crate) struct SilentMessages(Arc<Mutex<VecDeque<String>>>);

impl SilentMessages {
    pub(crate) fn record(&self, message_id: &str) {
        let mut ids = self.0.lock().unwrap();
        if ids.len() == LIMIT {
            ids.pop_front();
        }
        ids.push_back(message_id.to_owned());
    }

    /// Whether `message_id` was silent; it is forgotten once asked.
    pub(crate) fn take(&self, message_id: &str) -> bool {
        let mut ids = self.0.lock().unwrap();
        match ids.iter().position(|id| id == message_id) {
            Some(index) => {
                ids.remove(index);
                true
            }
            None => false,
        }
    }
}
