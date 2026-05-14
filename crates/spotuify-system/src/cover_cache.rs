//! Phase 14 (P14-B) — TTL-bounded, integrity-gated cover-art cache.
//!
//! Reused by media-controls (souvlaki wants a local file path),
//! notifications (notify-rust's `image_path`), and (future) TUI image
//! rendering. The cache must NEVER store placeholders or broken
//! images: a corrupted file on disk would propagate to every consumer
//! and make spotuify look misconfigured.
//!
//! ## Behaviour
//!
//! - **Cache dir**: `${cache_dir}/spotuify/covers/`.
//! - **Filename**: `sha256(url)[..32].<ext>` where `ext` comes from the
//!   HTTP `Content-Type` header (`image/jpeg → .jpg`, `png`, `webp`).
//!   Any other content type is rejected.
//! - **Integrity gate**: after fetch, decode with `image::load_from_memory`.
//!   Decode-fail → reject. Non-200 status → reject. Dimensions < 32px →
//!   reject. We treat "reject" as `Err(CoverCacheError::Broken)`; the
//!   caller renders a placeholder live, but the cache stays clean.
//! - **TTL refresh**: on every `get_or_fetch`, if the cached file is
//!   older than `ttl`, kick off a background refetch. Serve the stale
//!   file synchronously so MPRIS / notifications stay snappy.
//! - **LRU eviction by mtime**: when total bytes exceed `max_bytes`,
//!   delete the oldest-mtime files until we're under cap.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct CoverCacheConfig {
    pub root: PathBuf,
    pub ttl: Duration,
    pub max_bytes: u64,
}

impl Default for CoverCacheConfig {
    fn default() -> Self {
        let root = if cfg!(target_os = "macos") {
            dirs::home_dir()
                .map(|h| h.join("Library/Caches/spotuify/covers"))
                .unwrap_or_else(|| PathBuf::from("./spotuify-covers"))
        } else {
            dirs::cache_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
                .map(|d| d.join("spotuify/covers"))
                .unwrap_or_else(|| PathBuf::from("./spotuify-covers"))
        };
        Self {
            root,
            ttl: Duration::from_secs(7 * 24 * 60 * 60),
            max_bytes: 200 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoverCacheError {
    #[error("upstream returned non-success status {status}")]
    UpstreamStatus { status: u16 },
    #[error("upstream content-type `{actual}` is not a supported image type")]
    UnsupportedContentType { actual: String },
    #[error("image decode failed: {0}")]
    DecodeFailed(String),
    #[error("image dimensions too small ({width}x{height}); refusing to cache")]
    DimensionsTooSmall { width: u32, height: u32 },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct CoverCache {
    config: CoverCacheConfig,
    http: reqwest::Client,
    refresh_lock: Arc<Mutex<()>>,
}

impl CoverCache {
    pub fn new(config: CoverCacheConfig) -> Self {
        let _ = fs::create_dir_all(&config.root);
        let http = reqwest::Client::builder()
            .user_agent(format!(
                "spotuify/{} cover-cache",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config,
            http,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Best-effort: return a usable file path for `url`. Fetches if
    /// absent; refreshes in the background if older than `ttl` but
    /// serves the stale file synchronously.
    pub async fn get_or_fetch(&self, url: &str) -> Result<PathBuf, CoverCacheError> {
        if url.is_empty() {
            // Phase 14 (P14-B) — never cache a placeholder.
            return Err(CoverCacheError::UnsupportedContentType {
                actual: "<empty url>".to_string(),
            });
        }
        let key = hash_url(url);
        let existing = find_existing(&self.config.root, &key);
        if let Some(path) = existing.clone() {
            if !is_stale(&path, self.config.ttl) {
                return Ok(path);
            }
            // Stale: refetch in the background but serve the stale
            // file now. If the background refresh fails, the stale
            // file lives on; the next access will try again.
            self.spawn_background_refresh(url.to_string(), key);
            return Ok(path);
        }
        // First-time fetch: must complete (no stale to serve).
        self.fetch_and_persist(url, &key).await
    }

    fn spawn_background_refresh(&self, url: String, key: String) {
        let root = self.config.root.clone();
        let max_bytes = self.config.max_bytes;
        let http = self.http.clone();
        let lock = self.refresh_lock.clone();
        tokio::spawn(async move {
            let _guard = lock.lock().await;
            let outcome = fetch_and_persist_inner(&root, &http, &url, &key).await;
            match outcome {
                Ok(_) => evict_lru(&root, max_bytes).unwrap_or(()),
                Err(err) => tracing::debug!(
                    error = %err,
                    url = %url,
                    "background cover refresh failed; stale file kept"
                ),
            }
        });
    }

    async fn fetch_and_persist(
        &self,
        url: &str,
        key: &str,
    ) -> Result<PathBuf, CoverCacheError> {
        let path = fetch_and_persist_inner(&self.config.root, &self.http, url, key).await?;
        let _ = evict_lru(&self.config.root, self.config.max_bytes);
        Ok(path)
    }
}

async fn fetch_and_persist_inner(
    root: &Path,
    http: &reqwest::Client,
    url: &str,
    key: &str,
) -> Result<PathBuf, CoverCacheError> {
    let resp = http.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(CoverCacheError::UpstreamStatus {
            status: resp.status().as_u16(),
        });
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ext = extension_for_content_type(&content_type)
        .ok_or(CoverCacheError::UnsupportedContentType {
            actual: content_type.clone(),
        })?;
    let bytes = resp.bytes().await?;
    if bytes.is_empty() {
        return Err(CoverCacheError::DecodeFailed("empty body".to_string()));
    }
    // Integrity gate: must decode, must be non-tiny.
    let decoded = image::load_from_memory(&bytes)
        .map_err(|err| CoverCacheError::DecodeFailed(err.to_string()))?;
    let (w, h) = (decoded.width(), decoded.height());
    if w < 32 || h < 32 {
        return Err(CoverCacheError::DimensionsTooSmall {
            width: w,
            height: h,
        });
    }
    let path = root.join(format!("{key}.{ext}"));
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Atomic-ish replace: write to tmp then rename, so a concurrent
    // reader never sees a half-written file.
    let tmp = path.with_extension(format!("{ext}.tmp"));
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// LRU eviction by mtime when the cache exceeds `max_bytes`.
pub fn evict_lru(root: &Path, max_bytes: u64) -> std::io::Result<()> {
    let mut files: Vec<(SystemTime, PathBuf, u64)> = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !metadata.is_file() {
                continue;
            }
            let mtime = metadata.modified().unwrap_or(UNIX_EPOCH);
            files.push((mtime, entry.path(), metadata.len()));
        }
    }
    let total: u64 = files.iter().map(|(_, _, s)| *s).sum();
    if total <= max_bytes {
        return Ok(());
    }
    files.sort_by_key(|(t, _, _)| *t);
    let mut current = total;
    for (_, path, size) in files {
        if current <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            current = current.saturating_sub(size);
        }
    }
    Ok(())
}

fn find_existing(root: &Path, key: &str) -> Option<PathBuf> {
    for ext in ["jpg", "png", "webp"] {
        let path = root.join(format!("{key}.{ext}"));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn is_stale(path: &Path, ttl: Duration) -> bool {
    let mtime = match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true,
    };
    SystemTime::now()
        .duration_since(mtime)
        .map(|elapsed| elapsed > ttl)
        .unwrap_or(true)
}

fn hash_url(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(16).fold(String::new(), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(&mut acc, "{:02x}", b);
        acc
    })
}

fn extension_for_content_type(ct: &str) -> Option<&'static str> {
    let primary = ct.split(';').next().unwrap_or(ct).trim().to_ascii_lowercase();
    match primary.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_for_content_type_supports_jpeg_png_webp_and_rejects_others() {
        assert_eq!(extension_for_content_type("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for_content_type("image/png"), Some("png"));
        assert_eq!(extension_for_content_type("image/webp"), Some("webp"));
        // Spotify CDNs sometimes include a charset; we still parse the
        // primary type.
        assert_eq!(
            extension_for_content_type("image/jpeg; charset=binary"),
            Some("jpg")
        );
        // Anything else is rejected — preventing accidental caching of
        // an HTML error page.
        assert_eq!(extension_for_content_type("text/html"), None);
        assert_eq!(extension_for_content_type("application/json"), None);
        assert_eq!(extension_for_content_type(""), None);
    }

    #[test]
    fn hash_url_is_stable_and_short() {
        // The filename only uses the first 16 bytes of the sha256
        // (32 hex chars). That's plenty to avoid collisions in a
        // 200MB cache.
        let h = hash_url("https://i.scdn.co/image/abc");
        assert_eq!(h.len(), 32);
        assert_eq!(h, hash_url("https://i.scdn.co/image/abc"));
        assert_ne!(h, hash_url("https://i.scdn.co/image/def"));
    }

    #[test]
    fn is_stale_returns_true_when_file_missing() {
        // A non-existent path is "stale" so the caller refetches.
        let temp = tempfile::tempdir().unwrap();
        assert!(is_stale(
            &temp.path().join("ghost.jpg"),
            Duration::from_secs(60)
        ));
    }
}
