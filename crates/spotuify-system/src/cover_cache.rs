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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

const DEFAULT_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

type CoverFetchLock = Arc<Mutex<()>>;
type CoverFetchLocks = Arc<Mutex<HashMap<String, CoverFetchLock>>>;

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
            ttl: DEFAULT_TTL,
            max_bytes: 200 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverCacheEntry {
    pub path: PathBuf,
    pub cache_hit: bool,
    pub bytes: u64,
    pub fetched_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverCacheStats {
    pub root: PathBuf,
    pub files: u32,
    pub bytes: u64,
    pub oldest_entry_ms: Option<i64>,
    pub ttl_secs: u64,
    pub max_bytes: u64,
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
    in_flight: CoverFetchLocks,
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
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Best-effort: return a usable file path for `url`. Fetches if
    /// absent; refreshes in the background if older than `ttl` but
    /// serves the stale file synchronously.
    pub async fn get_or_fetch(&self, url: &str) -> Result<PathBuf, CoverCacheError> {
        Ok(self.get_or_fetch_entry(url).await?.path)
    }

    /// Like [`Self::get_or_fetch`], but returns cache-hit and file metadata for IPC clients.
    pub async fn get_or_fetch_entry(&self, url: &str) -> Result<CoverCacheEntry, CoverCacheError> {
        if url.is_empty() {
            // Phase 14 (P14-B) — never cache a placeholder.
            return Err(CoverCacheError::UnsupportedContentType {
                actual: "<empty url>".to_string(),
            });
        }
        let keys = cache_keys(url);
        let existing = find_existing(&self.config.root, &keys);
        if let Some(path) = existing.clone() {
            if !is_stale(&path, self.config.ttl) {
                return entry_for_path(path, true);
            }
            // Stale: refetch in the background but serve the stale
            // file now. If the background refresh fails, the stale
            // file lives on; the next access will try again.
            self.spawn_background_refresh(url.to_string(), keys.primary.clone());
            return entry_for_path(path, true);
        }
        // First-time fetch: must complete (no stale to serve).
        self.fetch_and_persist_deduped(url, &keys.primary).await
    }

    pub fn stats(&self) -> std::io::Result<CoverCacheStats> {
        stats_for_root(&self.config.root, self.config.ttl, self.config.max_bytes)
    }

    fn spawn_background_refresh(&self, url: String, key: String) {
        let root = self.config.root.clone();
        let max_bytes = self.config.max_bytes;
        let ttl = self.config.ttl;
        let http = self.http.clone();
        let in_flight = self.in_flight.clone();
        tokio::spawn(async move {
            let lock = lock_for_key(&in_flight, &key).await;
            let _guard = lock.lock().await;
            if let Some(path) = find_existing(&root, &CacheKeys::single(key.clone())) {
                if !is_stale(&path, ttl) {
                    return;
                }
            }
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

    async fn fetch_and_persist_deduped(
        &self,
        url: &str,
        key: &str,
    ) -> Result<CoverCacheEntry, CoverCacheError> {
        let lock = lock_for_key(&self.in_flight, key).await;
        let _guard = lock.lock().await;
        if let Some(path) = find_existing(&self.config.root, &CacheKeys::single(key.to_string())) {
            return entry_for_path(path, true);
        }
        let path = fetch_and_persist_inner(&self.config.root, &self.http, url, key).await?;
        let _ = evict_lru(&self.config.root, self.config.max_bytes);
        entry_for_path(path, false)
    }
}

async fn lock_for_key(locks: &CoverFetchLocks, key: &str) -> CoverFetchLock {
    let mut locks = locks.lock().await;
    locks
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
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
    let ext = extension_for_content_type(&content_type).ok_or(
        CoverCacheError::UnsupportedContentType {
            actual: content_type.clone(),
        },
    )?;
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

fn find_existing(root: &Path, keys: &CacheKeys) -> Option<PathBuf> {
    for key in keys.all() {
        for ext in ["jpg", "png", "webp"] {
            let path = root.join(format!("{key}.{ext}"));
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKeys {
    primary: String,
    legacy_hash: Option<String>,
}

impl CacheKeys {
    fn single(primary: String) -> Self {
        Self {
            primary,
            legacy_hash: None,
        }
    }

    fn all(&self) -> Vec<&str> {
        let mut keys = vec![self.primary.as_str()];
        if let Some(legacy_hash) = self.legacy_hash.as_deref() {
            keys.push(legacy_hash);
        }
        keys
    }
}

fn cache_keys(url: &str) -> CacheKeys {
    let legacy_hash = hash_url(url);
    match spotify_image_id(url) {
        Some(image_id) if image_id != legacy_hash => CacheKeys {
            primary: image_id,
            legacy_hash: Some(legacy_hash),
        },
        _ => CacheKeys::single(legacy_hash),
    }
}

fn spotify_image_id(url: &str) -> Option<String> {
    let (_, id) = url.rsplit_once("/image/")?;
    let id = id.split(['?', '#']).next().unwrap_or(id);
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return None;
    }
    Some(id.to_string())
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
    let primary = ct
        .split(';')
        .next()
        .unwrap_or(ct)
        .trim()
        .to_ascii_lowercase();
    match primary.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn entry_for_path(path: PathBuf, cache_hit: bool) -> Result<CoverCacheEntry, CoverCacheError> {
    let metadata = fs::metadata(&path)?;
    Ok(CoverCacheEntry {
        path,
        cache_hit,
        bytes: metadata.len(),
        fetched_at_ms: metadata.modified().ok().and_then(system_time_to_ms),
    })
}

fn stats_for_root(root: &Path, ttl: Duration, max_bytes: u64) -> std::io::Result<CoverCacheStats> {
    let mut files = 0_u32;
    let mut bytes = 0_u64;
    let mut oldest: Option<SystemTime> = None;
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => continue,
            };
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(metadata.len());
            if let Ok(modified) = metadata.modified() {
                oldest = Some(oldest.map_or(modified, |current| current.min(modified)));
            }
        }
    }
    Ok(CoverCacheStats {
        root: root.to_path_buf(),
        files,
        bytes,
        oldest_entry_ms: oldest.and_then(system_time_to_ms),
        ttl_secs: ttl.as_secs(),
        max_bytes,
    })
}

fn system_time_to_ms(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(root: PathBuf) -> CoverCacheConfig {
        CoverCacheConfig {
            root,
            ttl: Duration::from_secs(60),
            max_bytes: 10 * 1024 * 1024,
        }
    }

    fn png_bytes() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(40, 40)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("test PNG should encode");
        out.into_inner()
    }

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
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(is_stale(
            &temp.path().join("ghost.jpg"),
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn default_ttl_is_thirty_days() {
        assert_eq!(CoverCacheConfig::default().ttl, DEFAULT_TTL);
    }

    #[tokio::test]
    async fn fetched_spotify_images_use_image_id_filename() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/image/abc123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(png_bytes()),
            )
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let cache = CoverCache::new(test_config(temp.path().to_path_buf()));
        let entry = cache
            .get_or_fetch_entry(&format!("{}/image/abc123", server.uri()))
            .await
            .expect("image should fetch");

        assert!(!entry.cache_hit);
        assert_eq!(
            entry
                .path
                .file_name()
                .expect("cache path should have filename"),
            "abc123.png"
        );
        assert!(entry.bytes > 0);
        assert!(entry.fetched_at_ms.is_some());
    }

    #[tokio::test]
    async fn legacy_hashed_files_are_still_cache_hits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let url = "https://i.scdn.co/image/abc123";
        let legacy = temp.path().join(format!("{}.jpg", hash_url(url)));
        fs::write(&legacy, png_bytes()).expect("write legacy cache file");

        let cache = CoverCache::new(test_config(temp.path().to_path_buf()));
        let entry = cache
            .get_or_fetch_entry(url)
            .await
            .expect("legacy cache should hit");

        assert!(entry.cache_hit);
        assert_eq!(entry.path, legacy);
    }

    #[tokio::test]
    async fn stale_entries_are_served_while_refresh_runs_in_background() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/image/stale123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(png_bytes()),
            )
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let existing = temp.path().join("stale123.png");
        fs::write(&existing, png_bytes()).expect("write stale cache file");
        let cache = CoverCache::new(CoverCacheConfig {
            root: temp.path().to_path_buf(),
            ttl: Duration::ZERO,
            max_bytes: 10 * 1024 * 1024,
        });

        let entry = cache
            .get_or_fetch_entry(&format!("{}/image/stale123", server.uri()))
            .await
            .expect("stale cache should return existing entry");

        assert!(entry.cache_hit);
        assert_eq!(entry.path, existing);

        for _ in 0..20 {
            if !server
                .received_requests()
                .await
                .expect("wiremock should expose requests")
                .is_empty()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let final_requests = server
            .received_requests()
            .await
            .expect("wiremock should expose requests");
        assert!(
            !final_requests.is_empty(),
            "stale cache hit should trigger a background refresh"
        );
    }

    #[tokio::test]
    async fn concurrent_misses_share_one_download() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/image/dedupe123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(50))
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(png_bytes()),
            )
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let cache = Arc::new(CoverCache::new(test_config(temp.path().to_path_buf())));
        let url = format!("{}/image/dedupe123", server.uri());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let url = url.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_fetch_entry(&url)
                    .await
                    .expect("deduped fetch should succeed")
            }));
        }

        let mut paths = Vec::new();
        for handle in handles {
            paths.push(handle.await.expect("fetch task should join").path);
        }

        assert!(paths.iter().all(|path| path
            .file_name()
            .expect("cache path should have filename")
            == "dedupe123.png"));
        let calls = server
            .received_requests()
            .await
            .expect("wiremock should expose requests");
        assert_eq!(
            calls.len(),
            1,
            "concurrent misses must not stampede upstream"
        );
    }

    #[test]
    fn stats_report_cache_size_count_and_oldest_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("one.jpg"), [1_u8; 3]).expect("write one.jpg");
        fs::write(temp.path().join("two.png"), [2_u8; 5]).expect("write two.png");

        let cache = CoverCache::new(test_config(temp.path().to_path_buf()));
        let stats = cache.stats().expect("cache stats");

        assert_eq!(stats.files, 2);
        assert_eq!(stats.bytes, 8);
        assert_eq!(stats.ttl_secs, 60);
        assert_eq!(stats.max_bytes, 10 * 1024 * 1024);
        assert!(stats.oldest_entry_ms.is_some());
    }
}
