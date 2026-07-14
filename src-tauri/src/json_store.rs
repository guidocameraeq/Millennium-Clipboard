// Millennium Clipboard — atomic JSON store (Fase 2 · correctness)
//
// Generic persistence for the small JSON stores (preferences, settings,
// aliases, icon overrides, manual peers, clipboard sync). Fixes the two
// correctness bugs the copy-pasted stores shared:
//   1. In-place `fs::write` could leave a truncated file if the process
//      died mid-write (crash, power loss, zombie-kill). Every write now
//      goes to `<file>.tmp` and is `rename`d over the target — atomic on
//      the same volume, so a reader never sees a partial file.
//   2. A parse error used to `unwrap_or_default()`, silently wiping the
//      user's data. Now the raw bytes are backed up to `<file>.corrupt`,
//      an ERR is logged, and only THEN do we fall back to the default —
//      the loss becomes recoverable and diagnosable.
//
// Each store keeps its public API and inner type; only the I/O moves here.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct JsonStore<T> {
    path: PathBuf,
    inner: Mutex<T>,
    /// True when the file existed but failed to parse, so the in-memory
    /// state is the fallback default rather than the user's real data.
    /// Callers that take a destructive action from a loaded value (e.g.
    /// the autostart heal reading `start_with_windows`) must skip it here.
    loaded_corrupt: bool,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Send,
{
    /// Load `<data_dir>/<base>.<ext>` (honoring `MILLENNIUM_INSTANCE` for
    /// the dev double-launch), falling back to `default` when the file is
    /// missing or unparseable. On a parse error the raw bytes are copied
    /// to `<base>.<ext>.corrupt` and an ERR is logged before falling back.
    pub fn load_with_default(data_dir: &Path, base: &str, ext: &str, default: T) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("{base}-{s}.{ext}"),
            _ => format!("{base}.{ext}"),
        };
        let path = data_dir.join(filename);

        let mut loaded_corrupt = false;
        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            match serde_json::from_str::<T>(&raw) {
                Ok(v) => v,
                Err(e) => {
                    loaded_corrupt = true;
                    // `prefs.json` -> `prefs.json.corrupt` (with_extension
                    // replaces the trailing `.json`; passing `json.corrupt`
                    // keeps the original extension in the backup name).
                    let corrupt = path.with_extension(format!("{ext}.corrupt"));
                    match fs::write(&corrupt, &raw) {
                        Ok(()) => crate::runtime_log::err(format!(
                            "[jsonstore] parse failed for {} ({}). Backed up to {} and reset to default.",
                            path.display(),
                            e,
                            corrupt.display()
                        )),
                        Err(we) => crate::runtime_log::err(format!(
                            "[jsonstore] parse failed for {} ({}); could NOT back up to {}: {}. Falling back to default.",
                            path.display(),
                            e,
                            corrupt.display(),
                            we
                        )),
                    }
                    default
                }
            }
        } else {
            default
        };

        Ok(Self {
            path,
            inner: Mutex::new(inner),
            loaded_corrupt,
        })
    }

    /// True if the file existed but failed to parse (live state is the
    /// fallback default, not the user's real data).
    pub fn loaded_from_corrupt(&self) -> bool {
        self.loaded_corrupt
    }

    /// Mutate the state and persist it atomically. The `Mutex` is held across
    /// serialize + persist so two concurrent updates on the SAME store are
    /// fully serialized: the shared `<base>.tmp` can never be raced
    /// (write→rename→write→rename) and the committed on-disk state always
    /// matches the last mutation. This is safe — `persist()` is blocking
    /// filesystem I/O, NOT an `.await`, so the "never hold a std::sync::Mutex
    /// across an await" rule does not apply. Writes are tiny (small JSON), so
    /// the extra lock-hold is negligible.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R> {
        let mut guard = self.inner.lock().unwrap();
        let ret = f(&mut guard);
        let payload = serde_json::to_string_pretty(&*guard).context("serialize json store")?;
        self.persist(&payload)?;
        Ok(ret)
    }

    /// Read-only access to the state.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let guard = self.inner.lock().unwrap();
        f(&guard)
    }

    /// Write to `<file>.tmp` then rename over the destination. On Windows
    /// `fs::rename` maps to `MoveFileExW(.., MOVEFILE_REPLACE_EXISTING)`,
    /// which atomically replaces an existing file within the same volume,
    /// so a concurrent/next-boot reader never observes a partial file.
    fn persist(&self, payload: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, payload).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), self.path.display()))?;
        Ok(())
    }
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Default + Send,
{
    /// Convenience for stores whose inner type implements `Default`.
    pub fn load(data_dir: &Path, base: &str, ext: &str) -> Result<Self> {
        Self::load_with_default(data_dir, base, ext, T::default())
    }
}

// NOTE (Fase 2): these are pure fs+serde logic tests with zero GUI/Tauri
// surface, but they are gated off `windows` on purpose. Adding ANY test to
// this crate flips the MSVC linker's dead-code GC into *keeping* the full
// tao/wry windowing stack in the lib unit-test binary; that binary then
// statically imports comctl32-v6-only symbols (e.g. `TaskDialogIndirect`)
// WITHOUT the application manifest that `tauri-build` embeds into the real
// .exe, so the loader binds system32's comctl32 v5.82 and the process dies
// at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before any test
// runs. Keeping `cargo test` green on the primary platform matters more
// than running these here, where they can't launch anyway. They run on any
// non-Windows target, and are verified on Windows via the standalone
// harness described in the Fase 2 session notes. See docs/TODO.md.
#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct TestData {
        #[serde(default)]
        items: HashMap<String, String>,
    }

    /// Per-test scratch dir under the OS temp dir. Uniqueness comes from
    /// the pid + a per-test tag (no rng / clock, which the harness forbids
    /// in some contexts and which would make cleanup racy anyway).
    fn scratch(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "millennium-jsonstore-test-{}-{}",
            std::process::id(),
            tag
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trip_persists_and_reloads() {
        let dir = scratch("roundtrip");
        {
            let store = JsonStore::<TestData>::load(&dir, "rt", "json").unwrap();
            store
                .update(|d| {
                    d.items.insert("k".into(), "v".into());
                })
                .unwrap();
        }
        // A fresh load of the same path must observe the persisted value.
        let reloaded = JsonStore::<TestData>::load(&dir, "rt", "json").unwrap();
        assert_eq!(
            reloaded.read(|d| d.items.get("k").cloned()).as_deref(),
            Some("v")
        );
        assert!(!reloaded.loaded_from_corrupt());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_on_corrupt_preserves_original_and_defaults() {
        let dir = scratch("corrupt");
        let path = dir.join("bc.json");
        let bad = "{ this is not valid json";
        fs::write(&path, bad).unwrap();

        let store = JsonStore::<TestData>::load(&dir, "bc", "json").unwrap();
        // (a) falls back to default...
        assert_eq!(store.read(|d| d.items.len()), 0);
        assert!(store.loaded_from_corrupt());
        // (b) ...a .corrupt backup exists holding the ORIGINAL bytes...
        let corrupt = dir.join("bc.json.corrupt");
        assert!(corrupt.exists(), "expected {} to exist", corrupt.display());
        assert_eq!(fs::read_to_string(&corrupt).unwrap(), bad);
        // (c) ...and the original file is NOT deleted by load.
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_residual_tmp_after_update() {
        let dir = scratch("notmp");
        let store = JsonStore::<TestData>::load(&dir, "nt", "json").unwrap();
        store
            .update(|d| {
                d.items.insert("a".into(), "b".into());
            })
            .unwrap();
        assert!(
            !dir.join("nt.tmp").exists(),
            "residual .tmp left behind after update"
        );
        assert!(dir.join("nt.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
