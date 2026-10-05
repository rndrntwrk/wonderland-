//! Only validated acknowledged snapshots cross the local preview storage boundary.
use serde::{Deserialize, Serialize};
use wonderland_contracts::authoring::*;
pub const STORAGE_KEY: &str = "wonderland.authoring.v1";
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    snapshot: AuthoringProjection,
}
pub fn encode_snapshot(snapshot: &AuthoringProjection) -> Result<String, String> {
    snapshot.validate().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&Envelope {
        version: 1,
        snapshot: snapshot.clone(),
    })
    .map_err(|e| e.to_string())?;
    if json.len() > MAX_AUTHORING_JSON_BYTES {
        return Err("Local preview exceeds the save limit.".into());
    }
    Ok(json)
}
pub fn decode_snapshot(json: &str) -> Result<AuthoringProjection, String> {
    if json.len() > MAX_AUTHORING_JSON_BYTES {
        return Err("Saved preview exceeds the size limit.".into());
    }
    let envelope: Envelope =
        serde_json::from_str(json).map_err(|_| "Saved preview could not be read.".to_string())?;
    if envelope.version != 1 {
        return Err("Saved preview uses an unsupported version.".into());
    }
    envelope.snapshot.validate().map_err(|e| e.to_string())?;
    Ok(envelope.snapshot)
}

/// Per-load expectation of the exact localStorage envelope. Call compare_and_save only
/// while holding the browser's IndexedDB readwrite transaction across tabs.
#[derive(Debug)]
pub struct SaveSession {
    observed: Option<String>,
    writable: bool,
    notice: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    WriteFailed,
    Temporary,
    Cancelled,
}
impl SaveOutcome {
    pub fn announcement(self) -> &'static str {
        match self {
            Self::Saved => "Preview change saved on this device.",
            Self::WriteFailed => {
                "Preview change accepted for this session; it was not saved on this device. Try another change to retry saving."
            }
            Self::Temporary => {
                "Preview change accepted for this temporary session; it was not saved on this device."
            }
            Self::Cancelled => "",
        }
    }
}
impl SaveSession {
    pub fn new(observed: Option<String>) -> Self {
        Self {
            observed,
            writable: true,
            notice: String::new(),
        }
    }
    pub fn temporary(notice: impl Into<String>) -> Self {
        Self {
            observed: None,
            writable: false,
            notice: notice.into(),
        }
    }
    pub fn writable(&self) -> bool {
        self.writable
    }
    pub fn notice(&self) -> &str {
        &self.notice
    }
    pub fn disable(&mut self, notice: impl Into<String>) {
        self.writable = false;
        self.notice = notice.into();
    }
    pub fn write_failed(&mut self) -> SaveOutcome {
        self.notice = "Changes are available in this session but were not saved on this device. Try another change to retry saving.".into();
        SaveOutcome::WriteFailed
    }
    pub fn compare_and_save(
        &mut self,
        next: &str,
        read: impl FnOnce() -> Result<Option<String>, String>,
        write: impl FnOnce(&str) -> Result<(), String>,
    ) -> SaveOutcome {
        if !self.writable {
            return SaveOutcome::Temporary;
        }
        let current = match read() {
            Ok(value) => value,
            Err(_) => return self.write_failed(),
        };
        if current != self.observed {
            self.disable("Another tab changed the saved preview. Temporary preview; newer saved data is preserved. Reload to continue saving.");
            return SaveOutcome::Temporary;
        }
        if write(next).is_err() {
            return self.write_failed();
        }
        self.observed = Some(next.into());
        self.notice.clear();
        SaveOutcome::Saved
    }
}
#[cfg(target_arch = "wasm32")]
pub fn load() -> Result<(Option<AuthoringProjection>, SaveSession), String> {
    let storage = browser_storage()?;
    let raw = storage
        .get_item(STORAGE_KEY)
        .map_err(|_| "Local saving is unavailable.")?;
    let snapshot = raw.as_deref().map(decode_snapshot).transpose()?;
    Ok((snapshot, SaveSession::new(raw)))
}
#[cfg(target_arch = "wasm32")]
fn browser_storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Local saving is unavailable.")?
        .local_storage()
        .map_err(|_| "Local saving is unavailable.")?
        .ok_or_else(|| "Local saving is unavailable.".into())
}
#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    use wasm_bindgen::{JsCast, prelude::*};

    // All cooperating tabs use this one database/store scope. The get request's
    // success callback runs while the readwrite transaction is active. Its Rust
    // callback performs synchronous localStorage compare + set with no await.
    #[wasm_bindgen(inline_js = r#"
export function withAuthoringSaveTransaction(callback) {
  return new Promise((resolve, reject) => {
    let settled = false, db = null, tx = null, executed = false;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      if (tx && !executed) { try { tx.abort(); } catch (_) {} }
      if (db) db.close();
      // localStorage is synchronous and cannot roll back with IDB. If the
      // callback already ran, its result remains authoritative even on abort.
      if (error && !executed) reject(new Error(error)); else resolve();
    };
    const deadline = setTimeout(() => finish('Save coordination timed out'), 4000);
    let open;
    try { open = indexedDB.open('wonderland-authoring-save-coordinator', 1); }
    catch (_) { finish('Save coordination unavailable'); return; }
    open.onupgradeneeded = () => {
      if (!open.result.objectStoreNames.contains('serialize')) open.result.createObjectStore('serialize');
    };
    open.onerror = () => finish('Save coordination unavailable');
    open.onblocked = () => finish('Save coordination blocked');
    open.onsuccess = () => {
      db = open.result;
      if (settled) { db.close(); return; }
      db.onversionchange = () => { db.close(); finish('Save coordinator changed'); };
      try {
        tx = db.transaction('serialize', 'readwrite');
        tx.oncomplete = () => finish();
        tx.onabort = tx.onerror = () => finish('Save coordination failed');
        const request = tx.objectStore('serialize').get('mutex');
        request.onsuccess = () => {
          if (settled) return;
          try { callback(); executed = true; }
          catch (_) { finish('Save callback failed'); }
        };
      } catch (_) { finish('Save coordination unavailable'); }
    };
  });
}
"#)]
    extern "C" {
        #[wasm_bindgen(js_name = withAuthoringSaveTransaction)]
        fn coordinate(callback: &js_sys::Function) -> js_sys::Promise;
    }
    pub async fn save(
        session: Arc<std::sync::Mutex<SaveSession>>,
        snapshot: AuthoringProjection,
        alive: Arc<AtomicBool>,
    ) -> SaveOutcome {
        if !alive.load(Ordering::Acquire) {
            return SaveOutcome::Cancelled;
        }
        if !session.lock().expect("save session").writable() {
            return SaveOutcome::Temporary;
        }
        let json = match encode_snapshot(&snapshot) {
            Ok(json) => json,
            Err(_) => return session.lock().expect("save session").write_failed(),
        };
        let outcome = Rc::new(RefCell::new(None));
        let result = outcome.clone();
        let writer = session.clone();
        let active = alive.clone();
        let callback = Closure::<dyn FnMut()>::new(move || {
            if !active.load(Ordering::Acquire) {
                *result.borrow_mut() = Some(SaveOutcome::Cancelled);
                return;
            }
            let saved = match browser_storage() {
                Ok(storage) => writer.lock().expect("save session").compare_and_save(
                    &json,
                    || {
                        storage
                            .get_item(STORAGE_KEY)
                            .map_err(|_| "Read failed".into())
                    },
                    |next| {
                        storage
                            .set_item(STORAGE_KEY, next)
                            .map_err(|_| "Write failed".into())
                    },
                ),
                Err(_) => writer.lock().expect("save session").write_failed(),
            };
            *result.borrow_mut() = Some(saved);
        });
        let coordinated =
            wasm_bindgen_futures::JsFuture::from(coordinate(callback.as_ref().unchecked_ref()))
                .await;
        drop(callback);
        if !alive.load(Ordering::Acquire) {
            return SaveOutcome::Cancelled;
        }
        if let Some(saved) = *outcome.borrow() {
            return saved;
        }
        if coordinated.is_err() {
            session.lock().expect("save session").disable("Saving coordination is unavailable. Temporary preview; existing data is preserved. Reload to retry saving.");
        }
        SaveOutcome::Temporary
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::save;
