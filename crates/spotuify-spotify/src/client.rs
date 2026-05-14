use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::Mutex;

use spotuify_core::{now_ms, spotify_api_finished_event, AnalyticsEvent, AnalyticsSource};

use crate::auth::{self, StoredToken};
use crate::config::Config;

// Re-export domain types from spotuify-core so existing call sites
// (`crate::spotify::Playback`, etc.) keep working.
pub use spotuify_core::{Device, MediaItem, MediaKind, Playback, Playlist, Queue};

const API: &str = "https://api.spotify.com/v1";

/// Phase 13 (P13-E) — canonical User-Agent attached to every outbound
/// HTTP request. The OS+arch suffix lets Spotify operations triage
/// platform-specific issues; the GitHub URL is etiquette for any
/// third-party endpoints we hit (LRCLIB, image CDNs, etc.).
pub fn user_agent_string() -> String {
    format!(
        "spotuify/{version} ({os}; {arch}; +https://github.com/planetaryescape/spotuify)",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
    )
}

#[derive(Clone)]
pub struct SpotifyClient {
    config: Config,
    http: Client,
    /// Decoupled via `spotuify_core::AnalyticsSink` so any
    /// Send+Sync+Debug impl works -- the binary's `AnalyticsStore`
    /// is one; tests and future crates can supply their own.
    analytics: Option<Arc<dyn spotuify_core::AnalyticsSink>>,
    analytics_source: AnalyticsSource,
    fake: bool,
    token_cache: Arc<Mutex<Option<StoredToken>>>,
}

impl SpotifyClient {
    pub fn new(config: Config) -> Result<Self> {
        let http = Client::builder()
            .user_agent(user_agent_string())
            .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .connect_timeout(Duration::from_secs(4))
            .read_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(8))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            config,
            http,
            analytics: None,
            analytics_source: AnalyticsSource::Cli,
            fake: false,
            token_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub fn fake() -> Result<Self> {
        let config = Config {
            client_id: "fake-client-id".to_string(),
            client_secret: Some("fake-client-secret".to_string()),
            redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
            config_path: PathBuf::from("fake-spotuify.toml"),
            spotifyd_config_path: PathBuf::from("fake-spotifyd.conf"),
            spotifyd_device_name: Some("spotuify-fake".to_string()),
            spotifyd_autostart: false,
            player: crate::config::PlayerConfig::default(),
            analytics: crate::config::AnalyticsConfig::default(),
        };
        Ok(Self::new(config)?.with_fake_backend())
    }

    fn with_fake_backend(mut self) -> Self {
        self.fake = true;
        self
    }

    pub fn with_analytics(
        mut self,
        analytics: Arc<dyn spotuify_core::AnalyticsSink>,
        source: AnalyticsSource,
    ) -> Self {
        self.analytics = Some(analytics);
        self.analytics_source = source;
        self
    }

    pub fn with_token_cache(mut self, token_cache: Arc<Mutex<Option<StoredToken>>>) -> Self {
        self.token_cache = token_cache;
        self
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn analytics_source(&self) -> AnalyticsSource {
        self.analytics_source
    }

    pub async fn record_analytics_event(&self, event: AnalyticsEvent) {
        let Some(analytics) = &self.analytics else {
            return;
        };
        // AnalyticsSink::record swallows failures inside the impl per
        // the trait contract -- analytics is best-effort and must not
        // block the producer.
        analytics.record(&event).await;
    }

    async fn record_spotify_api_finished(
        &self,
        method: &Method,
        path: &str,
        status: Option<StatusCode>,
        elapsed_ms: u128,
        error_class: Option<&str>,
    ) {
        self.record_analytics_event(spotify_api_finished_event(
            AnalyticsSource::SpotifyApi,
            method.as_str(),
            path,
            status.map(|status| status.as_u16()),
            elapsed_ms,
            error_class,
            now_ms(),
        ))
        .await;
    }

    pub async fn playback(&mut self) -> Result<Playback> {
        if self.fake {
            return Ok(fake_playback());
        }
        match self
            .request_json::<PlaybackResponse>(Method::GET, "/me/player", None::<()>)
            .await
        {
            Ok(Some(raw)) => Ok(raw.into_playback()),
            Ok(None) => Ok(Playback::default()),
            Err(err) => Err(err),
        }
    }

    pub async fn devices(&mut self) -> Result<Vec<Device>> {
        if self.fake {
            return Ok(vec![fake_device()]);
        }
        let response = self
            .request_json::<DevicesResponse>(Method::GET, "/me/player/devices", None::<()>)
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no devices response"))?;
        Ok(response.devices)
    }

    pub async fn queue(&mut self) -> Result<Queue> {
        if self.fake {
            return Ok(Queue {
                currently_playing: Some(fake_track()),
                items: vec![fake_second_track()],
            });
        }
        let response = self
            .request_json::<QueueResponse>(Method::GET, "/me/player/queue", None::<()>)
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no queue response"))?;
        Ok(Queue {
            currently_playing: response
                .currently_playing
                .and_then(RawPlayable::into_media_item),
            items: response
                .queue
                .into_iter()
                .filter_map(RawPlayable::into_media_item)
                .collect(),
        })
    }

    pub async fn search_with_limit(
        &mut self,
        query: &str,
        kinds: &[MediaKind],
        limit: u8,
    ) -> Result<Vec<MediaItem>> {
        if self.fake {
            return Ok(fake_search_results(query, kinds, limit));
        }
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let path = search_path(query, kinds, limit);
        let response = self
            .request_json::<SearchResponse>(Method::GET, &path, None::<()>)
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no search response"))?;

        let mut items = Vec::new();
        if let Some(tracks) = response.tracks {
            items.extend(tracks.items.into_iter().map(RawTrack::into_media_item));
        }
        if let Some(episodes) = response.episodes {
            items.extend(episodes.items.into_iter().map(RawEpisode::into_media_item));
        }
        if let Some(albums) = response.albums {
            items.extend(albums.items.into_iter().map(RawAlbum::into_media_item));
        }
        if let Some(artists) = response.artists {
            items.extend(artists.items.into_iter().map(RawArtist::into_media_item));
        }
        if let Some(playlists) = response.playlists {
            items.extend(
                playlists
                    .items
                    .into_iter()
                    .flatten()
                    .filter_map(RawPlaylist::into_media_item),
            );
        }

        Ok(items)
    }

    pub async fn playlists(&mut self) -> Result<Vec<Playlist>> {
        if self.fake {
            return Ok(fake_playlists());
        }
        let mut offset = 0;
        let mut playlists = Vec::new();
        loop {
            let path = format!("/me/playlists?limit=50&offset={offset}");
            let response = self
                .request_json::<Paging<Option<RawPlaylist>>>(Method::GET, &path, None::<()>)
                .await?
                .ok_or_else(|| anyhow!("Spotify returned no playlists response"))?;
            let total = response.total;
            playlists.extend(
                response
                    .items
                    .into_iter()
                    .flatten()
                    .filter_map(RawPlaylist::into_playlist),
            );
            offset += 50;
            if offset >= total || playlists.len() >= 250 {
                break;
            }
        }
        Ok(playlists)
    }

    pub async fn current_user_id(&mut self) -> Result<String> {
        if self.fake {
            return Ok("fake-user".to_string());
        }
        let response = self
            .request_json::<CurrentUserResponse>(Method::GET, "/me", None::<()>)
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no current user response"))?;
        Ok(response.id)
    }

    pub async fn create_playlist(
        &mut self,
        name: &str,
        description: Option<&str>,
        public: bool,
    ) -> Result<Playlist> {
        if self.fake {
            return Ok(Playlist {
                id: fake_playlist_id(name),
                name: name.to_string(),
                owner: "Fake User".to_string(),
                tracks_total: 0,
                image_url: None,
                snapshot_id: None,
            });
        }
        let user_id = self.current_user_id().await?;
        let user_id = encode_component(&user_id);
        let body = serde_json::json!({
            "name": name,
            "description": description.unwrap_or("Created by spotuify"),
            "public": public,
        });
        self.request_json::<RawPlaylist>(
            Method::POST,
            &format!("/users/{user_id}/playlists"),
            Some(body),
        )
        .await?
        .and_then(RawPlaylist::into_playlist)
        .ok_or_else(|| anyhow!("Spotify returned no created playlist"))
    }

    pub async fn recently_played(&mut self) -> Result<Vec<MediaItem>> {
        if self.fake {
            return Ok(vec![fake_track(), fake_second_track()]);
        }
        let response = self
            .request_json::<RecentlyPlayedResponse>(
                Method::GET,
                "/me/player/recently-played?limit=20",
                None::<()>,
            )
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no recently played response"))?;
        Ok(response
            .items
            .into_iter()
            .map(|item| item.track.into_media_item())
            .collect())
    }

    pub async fn saved_tracks(&mut self) -> Result<Vec<MediaItem>> {
        if self.fake {
            return Ok(vec![fake_track(), fake_second_track()]);
        }
        let mut offset = 0;
        let mut items = Vec::new();
        loop {
            let path = format!("/me/tracks?limit=50&offset={offset}");
            let response = self
                .request_json::<Paging<SavedTrackItem>>(Method::GET, &path, None::<()>)
                .await?
                .ok_or_else(|| anyhow!("Spotify returned no saved tracks response"))?;
            let total = response.total;
            items.extend(
                response
                    .items
                    .into_iter()
                    .map(|item| item.track.into_media_item()),
            );
            offset += 50;
            if offset >= total || items.len() >= 250 {
                break;
            }
        }
        Ok(items)
    }

    pub async fn saved_albums(&mut self) -> Result<Vec<MediaItem>> {
        if self.fake {
            return Ok(vec![fake_album()]);
        }
        let mut offset = 0;
        let mut items = Vec::new();
        loop {
            let path = format!("/me/albums?limit=50&offset={offset}");
            let response = self
                .request_json::<Paging<SavedAlbumItem>>(Method::GET, &path, None::<()>)
                .await?
                .ok_or_else(|| anyhow!("Spotify returned no saved albums response"))?;
            let total = response.total;
            items.extend(
                response
                    .items
                    .into_iter()
                    .map(|item| item.album.into_media_item()),
            );
            offset += 50;
            if offset >= total || items.len() >= 250 {
                break;
            }
        }
        Ok(items)
    }

    pub async fn playlist_tracks(&mut self, playlist_id: &str) -> Result<Vec<MediaItem>> {
        if self.fake {
            if fake_playlists()
                .iter()
                .any(|playlist| playlist.id == playlist_id)
            {
                return Ok(vec![fake_track(), fake_second_track()]);
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        let mut offset = 0;
        let mut tracks = Vec::new();
        loop {
            let path = format!("/playlists/{playlist_id}/tracks?limit=50&offset={offset}");
            let response = self
                .request_json::<Paging<PlaylistTrackItem>>(Method::GET, &path, None::<()>)
                .await?
                .ok_or_else(|| anyhow!("Spotify returned no playlist tracks response"))?;
            let total = response.total;
            tracks.extend(
                response
                    .items
                    .into_iter()
                    .filter_map(|item| item.track.into_media_item()),
            );
            offset += 50;
            if offset >= total || tracks.len() >= 500 {
                break;
            }
        }
        Ok(tracks)
    }

    pub async fn play_pause(&mut self, is_playing: bool) -> Result<()> {
        if self.fake {
            let _ = is_playing;
            return Ok(());
        }
        if is_playing {
            self.empty(Method::PUT, "/me/player/pause", None::<()>)
                .await
        } else {
            self.empty(Method::PUT, "/me/player/play", Some(serde_json::json!({})))
                .await
        }
    }

    pub async fn play_uri(&mut self, uri: &str, kind: &MediaKind) -> Result<()> {
        if self.fake {
            let _ = (uri, kind);
            return Ok(());
        }
        let body = match kind {
            MediaKind::Album | MediaKind::Artist | MediaKind::Playlist => {
                serde_json::json!({ "context_uri": uri })
            }
            _ => serde_json::json!({ "uris": [uri] }),
        };
        self.empty(Method::PUT, "/me/player/play", Some(body)).await
    }

    pub async fn next(&mut self) -> Result<()> {
        if self.fake {
            return Ok(());
        }
        self.empty(Method::POST, "/me/player/next", None::<()>)
            .await
    }

    pub async fn previous(&mut self) -> Result<()> {
        if self.fake {
            return Ok(());
        }
        self.empty(Method::POST, "/me/player/previous", None::<()>)
            .await
    }

    pub async fn seek(&mut self, position_ms: u64) -> Result<()> {
        if self.fake {
            let _ = position_ms;
            return Ok(());
        }
        self.empty(
            Method::PUT,
            &format!("/me/player/seek?position_ms={position_ms}"),
            None::<()>,
        )
        .await
    }

    pub async fn volume(&mut self, volume_percent: u8) -> Result<()> {
        if self.fake {
            let _ = volume_percent;
            return Ok(());
        }
        let volume_percent = volume_percent.min(100);
        self.empty(
            Method::PUT,
            &format!("/me/player/volume?volume_percent={volume_percent}"),
            None::<()>,
        )
        .await
    }

    pub async fn shuffle(&mut self, state: bool) -> Result<()> {
        if self.fake {
            let _ = state;
            return Ok(());
        }
        self.empty(
            Method::PUT,
            &format!("/me/player/shuffle?state={state}"),
            None::<()>,
        )
        .await
    }

    pub async fn repeat(&mut self, state: &str) -> Result<()> {
        if self.fake {
            let _ = state;
            return Ok(());
        }
        self.empty(
            Method::PUT,
            &format!("/me/player/repeat?state={state}"),
            None::<()>,
        )
        .await
    }

    pub async fn add_to_queue(&mut self, uri: &str) -> Result<()> {
        if self.fake {
            selection_like_uri_check(uri)?;
            return Ok(());
        }
        let encoded = url::form_urlencoded::byte_serialize(uri.as_bytes()).collect::<String>();
        self.empty(
            Method::POST,
            &format!("/me/player/queue?uri={encoded}"),
            None::<()>,
        )
        .await
    }

    pub async fn transfer(&mut self, device_id: &str, play: bool) -> Result<()> {
        if self.fake {
            let _ = play;
            if fake_device().id.as_deref() == Some(device_id) || device_id == "spotuify-fake" {
                return Ok(());
            }
            bail!("no device matching `{device_id}`");
        }
        self.empty(
            Method::PUT,
            "/me/player",
            Some(serde_json::json!({ "device_ids": [device_id], "play": play })),
        )
        .await
    }

    pub async fn add_to_playlist(&mut self, playlist_id: &str, uri: &str) -> Result<()> {
        self.add_items_to_playlist(playlist_id, &[uri.to_string()])
            .await
    }

    pub async fn add_items_to_playlist(
        &mut self,
        playlist_id: &str,
        uris: &[String],
    ) -> Result<()> {
        if self.fake {
            if fake_playlists()
                .iter()
                .any(|playlist| playlist.id == playlist_id)
            {
                for uri in uris {
                    selection_like_uri_check(uri)?;
                }
                return Ok(());
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        if uris.is_empty() {
            return Ok(());
        }
        let playlist_id = encode_component(playlist_id);
        for chunk in uris.chunks(100) {
            self.empty(
                Method::POST,
                &format!("/playlists/{playlist_id}/tracks"),
                Some(serde_json::json!({ "uris": chunk })),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn save_item(&mut self, item: &MediaItem) -> Result<()> {
        if self.fake {
            selection_like_uri_check(&item.uri)?;
            return Ok(());
        }
        let id = item
            .id
            .as_deref()
            .ok_or_else(|| anyhow!("selected item has no Spotify id"))?;
        match item.kind {
            MediaKind::Track => {
                self.empty(Method::PUT, &format!("/me/tracks?ids={id}"), None::<()>)
                    .await
            }
            MediaKind::Episode => {
                self.empty(Method::PUT, &format!("/me/episodes?ids={id}"), None::<()>)
                    .await
            }
            _ => bail!("only tracks and episodes can be saved from now playing"),
        }
    }

    // ---------------------------------------------------------------
    // Phase 12 (P12-A) — inverse mutators used by `apply_reversal`.
    //
    // Each method delegates URL+body shape to a pure helper at the
    // bottom of the file so the wire format stays unit-testable.
    // ---------------------------------------------------------------

    /// `DELETE /v1/playlists/{id}/tracks` with `tracks[].uri` and
    /// optional `snapshot_id` precondition. Returns the new
    /// `snapshot_id` Spotify hands back so the caller can persist it.
    pub async fn remove_playlist_items(
        &mut self,
        playlist_id: &str,
        uris: &[String],
        snapshot_id: Option<&str>,
    ) -> Result<String> {
        if self.fake {
            if fake_playlists().iter().any(|p| p.id == playlist_id) {
                return Ok("fake-snap-after-remove".to_string());
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        if uris.is_empty() {
            // No-op remove still needs a snapshot to return; surface the
            // caller's stored one (best-effort) or empty so the caller
            // can decide not to persist.
            return Ok(snapshot_id.unwrap_or_default().to_string());
        }
        let encoded = encode_component(playlist_id);
        let mut current_snapshot = snapshot_id.map(str::to_string);
        for chunk in uris.chunks(100) {
            let body = playlist_remove_items_body(chunk, current_snapshot.as_deref());
            let resp = self
                .request_json::<SnapshotResponse>(
                    Method::DELETE,
                    &format!("/playlists/{encoded}/tracks"),
                    Some(body),
                )
                .await?
                .ok_or_else(|| anyhow!("Spotify returned no response for playlist-remove"))?;
            current_snapshot = Some(resp.snapshot_id);
        }
        current_snapshot.ok_or_else(|| anyhow!("Spotify returned no snapshot_id"))
    }

    /// Re-add items at their original positions (undo of a previous
    /// remove). Groups by position so each unique position becomes one
    /// `POST /v1/playlists/{id}/tracks?position={p}` call carrying the
    /// URIs that landed at that position.
    pub async fn add_items_to_playlist_at_positions(
        &mut self,
        playlist_id: &str,
        items: &[(String, u32)],
        snapshot_id: Option<&str>,
    ) -> Result<String> {
        let _ = snapshot_id; // Spotify's add endpoint ignores snapshot_id.
        if self.fake {
            if fake_playlists().iter().any(|p| p.id == playlist_id) {
                return Ok("fake-snap-after-readd".to_string());
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        if items.is_empty() {
            return Ok(String::new());
        }
        let encoded = encode_component(playlist_id);
        let groups = group_items_by_position(items);
        let mut last_snapshot = String::new();
        for (position, uris) in groups {
            for chunk in uris.chunks(100) {
                let body = serde_json::json!({ "uris": chunk });
                let resp = self
                    .request_json::<SnapshotResponse>(
                        Method::POST,
                        &format!("/playlists/{encoded}/tracks?position={position}"),
                        Some(body),
                    )
                    .await?
                    .ok_or_else(|| anyhow!("Spotify returned no response for playlist-add"))?;
                last_snapshot = resp.snapshot_id;
            }
        }
        Ok(last_snapshot)
    }

    /// Reorder a contiguous range of items in a playlist.
    /// `PUT /v1/playlists/{id}/tracks` with `{range_start, range_length,
    /// insert_before, snapshot_id?}`.
    pub async fn reorder_playlist_items(
        &mut self,
        playlist_id: &str,
        range_start: u32,
        insert_before: u32,
        range_length: u32,
        snapshot_id: Option<&str>,
    ) -> Result<String> {
        if self.fake {
            if fake_playlists().iter().any(|p| p.id == playlist_id) {
                return Ok("fake-snap-after-reorder".to_string());
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        let encoded = encode_component(playlist_id);
        let body =
            playlist_reorder_body(range_start, insert_before, range_length, snapshot_id);
        let resp = self
            .request_json::<SnapshotResponse>(
                Method::PUT,
                &format!("/playlists/{encoded}/tracks"),
                Some(body),
            )
            .await?
            .ok_or_else(|| anyhow!("Spotify returned no response for playlist-reorder"))?;
        Ok(resp.snapshot_id)
    }

    /// Unfollow / delete a playlist. Spotify models playlist deletion
    /// as the owner unfollowing it. `DELETE /v1/playlists/{id}/followers`.
    pub async fn unfollow_playlist(&mut self, playlist_id: &str) -> Result<()> {
        if self.fake {
            if fake_playlists().iter().any(|p| p.id == playlist_id) {
                return Ok(());
            }
            bail!("no playlist matching `{playlist_id}`");
        }
        let encoded = encode_component(playlist_id);
        self.empty(
            Method::DELETE,
            &format!("/playlists/{encoded}/followers"),
            None::<()>,
        )
        .await
    }

    /// Save (=like) an item by URI. Routes to the correct
    /// `/me/{tracks,albums,episodes,shows}` endpoint based on the URI
    /// kind and uses Spotify's `?ids=` query syntax.
    pub async fn library_save_by_uri(&mut self, uri: &str) -> Result<()> {
        if self.fake {
            selection_like_uri_check(uri)?;
            return Ok(());
        }
        let (path, _id) = library_endpoint_for_uri(uri)?;
        self.empty(Method::PUT, &path, None::<()>).await
    }

    /// Inverse of `library_save_by_uri`. `DELETE` against the same
    /// endpoint family.
    pub async fn library_unsave_by_uri(&mut self, uri: &str) -> Result<()> {
        if self.fake {
            selection_like_uri_check(uri)?;
            return Ok(());
        }
        let (path, _id) = library_endpoint_for_uri(uri)?;
        self.empty(Method::DELETE, &path, None::<()>).await
    }

    pub async fn image(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("image request failed")?;
        let status = response.status();
        if !status.is_success() {
            bail!("image request failed with {status}");
        }
        Ok(response
            .bytes()
            .await
            .context("failed to read image")?
            .to_vec())
    }

    async fn empty<T: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: Option<T>,
    ) -> Result<()> {
        let token = auth::access_token_cached(&self.config, &self.http, &self.token_cache).await?;
        let url = format!("{API}{path}");
        let started = Instant::now();
        tracing::debug!(method = %method, path, "Spotify request start");
        let mut request = self.http.request(method.clone(), url).bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                self.record_spotify_api_finished(
                    &method,
                    path,
                    None,
                    started.elapsed().as_millis(),
                    Some("transport"),
                )
                .await;
                tracing::warn!(method = %method, path, error = %err, "Spotify request send failed");
                return Err(err).with_context(|| format!("Spotify {method} {path} request failed"));
            }
        };
        let status = response.status();
        self.record_spotify_api_finished(
            &method,
            path,
            Some(status),
            started.elapsed().as_millis(),
            if status.is_success() {
                None
            } else {
                Some("http")
            },
        )
        .await;
        tracing::debug!(
            method = %method,
            path,
            status = %status,
            elapsed_ms = started.elapsed().as_millis(),
            "Spotify request finished"
        );
        handle_empty_response(&method, path, response).await
    }

    async fn request_json<T: DeserializeOwned>(
        &mut self,
        method: Method,
        path: &str,
        body: Option<impl Serialize>,
    ) -> Result<Option<T>> {
        let token = auth::access_token_cached(&self.config, &self.http, &self.token_cache).await?;
        let url = format!("{API}{path}");
        let started = Instant::now();
        tracing::debug!(method = %method, path, "Spotify request start");
        let mut request = self.http.request(method.clone(), url).bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                self.record_spotify_api_finished(
                    &method,
                    path,
                    None,
                    started.elapsed().as_millis(),
                    Some("transport"),
                )
                .await;
                tracing::warn!(method = %method, path, error = %err, "Spotify request send failed");
                return Err(err).with_context(|| format!("Spotify {method} {path} request failed"));
            }
        };
        let status = response.status();
        self.record_spotify_api_finished(
            &method,
            path,
            Some(status),
            started.elapsed().as_millis(),
            if status.is_success() {
                None
            } else {
                Some("http")
            },
        )
        .await;
        tracing::debug!(
            method = %method,
            path,
            status = %status,
            elapsed_ms = started.elapsed().as_millis(),
            "Spotify request finished"
        );
        handle_json_response(&method, path, response).await
    }
}

fn search_path(query: &str, kinds: &[MediaKind], limit: u8) -> String {
    let encoded = encode_component(query);
    let types = kinds
        .iter()
        .map(MediaKind::label)
        .collect::<Vec<_>>()
        .join(",");
    let limit = limit.min(10);
    format!("/search?q={encoded}&type={types}&limit={limit}")
}

fn encode_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
}

async fn handle_empty_response(
    method: &Method,
    path: &str,
    response: reqwest::Response,
) -> Result<()> {
    let status = response.status();
    if status.is_success() || status == StatusCode::NO_CONTENT {
        return Ok(());
    }

    let retry = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response.text().await.unwrap_or_default();
    if let Some(retry) = retry {
        bail!("Spotify {method} {path} was rate limited; retry after {retry}s");
    }
    let message = spotify_error_message(&body);
    tracing::warn!(method = %method, path, status = %status, body = %trim_body(&body), "Spotify request failed");
    bail!("Spotify {method} {path} failed ({status}): {message}")
}

async fn handle_json_response<T: DeserializeOwned>(
    method: &Method,
    path: &str,
    response: reqwest::Response,
) -> Result<Option<T>> {
    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        return Ok(None);
    }
    if !status.is_success() {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.text().await.unwrap_or_default();
        if let Some(retry) = retry {
            bail!("Spotify {method} {path} was rate limited; retry after {retry}s");
        }
        let message = spotify_error_message(&body);
        tracing::warn!(method = %method, path, status = %status, body = %trim_body(&body), "Spotify request failed");
        bail!("Spotify {method} {path} failed ({status}): {message}");
    }
    let body = response
        .text()
        .await
        .with_context(|| format!("failed to read Spotify {method} {path} response"))?;
    match serde_json::from_str::<T>(&body) {
        Ok(value) => Ok(Some(value)),
        Err(err) => {
            tracing::warn!(
                method = %method,
                path,
                error = %err,
                body = %trim_body(&body),
                "failed to decode Spotify response"
            );
            Err(err).with_context(|| format!("failed to decode Spotify {method} {path} response"))
        }
    }
}

fn spotify_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(|message| message.as_str())
                .or_else(|| {
                    value
                        .get("error_description")
                        .and_then(|message| message.as_str())
                })
                .map(str::to_string)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| trim_body(body))
}

fn trim_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return "empty response body".to_string();
    }
    const MAX: usize = 500;
    if body.len() <= MAX {
        body.to_string()
    } else {
        format!("{}...", &body[..MAX])
    }
}

#[derive(Debug, Deserialize)]
struct PlaybackResponse {
    device: Option<Device>,
    repeat_state: Option<String>,
    shuffle_state: Option<bool>,
    progress_ms: Option<u64>,
    is_playing: Option<bool>,
    item: Option<RawPlayable>,
}

impl PlaybackResponse {
    fn into_playback(self) -> Playback {
        Playback {
            item: self.item.and_then(RawPlayable::into_media_item),
            device: self.device,
            is_playing: self.is_playing.unwrap_or(false),
            progress_ms: self.progress_ms.unwrap_or_default(),
            shuffle: self.shuffle_state.unwrap_or(false),
            repeat: self.repeat_state.unwrap_or_else(|| "off".to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize)]
struct QueueResponse {
    currently_playing: Option<RawPlayable>,
    queue: Vec<RawPlayable>,
}

#[derive(Debug, Deserialize)]
struct CurrentUserResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RecentlyPlayedResponse {
    items: Vec<RecentlyPlayedItem>,
}

#[derive(Debug, Deserialize)]
struct RecentlyPlayedItem {
    track: RawTrack,
}

#[derive(Debug, Deserialize)]
struct SavedTrackItem {
    track: RawTrack,
}

#[derive(Debug, Deserialize)]
struct SavedAlbumItem {
    album: RawAlbum,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    tracks: Option<Paging<RawTrack>>,
    episodes: Option<Paging<RawEpisode>>,
    albums: Option<Paging<RawAlbum>>,
    artists: Option<Paging<RawArtist>>,
    playlists: Option<Paging<Option<RawPlaylist>>>,
}

#[derive(Debug, Deserialize)]
struct Paging<T> {
    items: Vec<T>,
    total: u64,
}

#[derive(Debug, Deserialize)]
struct PlaylistTrackItem {
    track: RawPlayable,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
enum RawPlayable {
    #[serde(rename = "track")]
    Track(RawTrack),
    #[serde(rename = "episode")]
    Episode(RawEpisode),
    #[serde(other)]
    Other,
}

impl RawPlayable {
    fn into_media_item(self) -> Option<MediaItem> {
        match self {
            Self::Track(track) => Some(track.into_media_item()),
            Self::Episode(episode) => Some(episode.into_media_item()),
            Self::Other => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawTrack {
    id: Option<String>,
    uri: String,
    name: String,
    duration_ms: u64,
    explicit: Option<bool>,
    is_playable: Option<bool>,
    #[serde(default, deserialize_with = "null_to_default")]
    artists: Vec<SimpleNamed>,
    album: RawAlbum,
}

impl RawTrack {
    fn into_media_item(self) -> MediaItem {
        let artists = join_names(&self.artists);
        MediaItem {
            id: self.id,
            uri: self.uri,
            name: self.name,
            subtitle: artists,
            context: self.album.name.clone(),
            duration_ms: self.duration_ms,
            image_url: image_url(&self.album.images),
            kind: MediaKind::Track,
            source: Some("spotify".to_string()),
            freshness: None,
            explicit: self.explicit,
            is_playable: self.is_playable,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawEpisode {
    id: Option<String>,
    uri: String,
    name: String,
    duration_ms: u64,
    show: Option<SimpleShow>,
    #[serde(default, deserialize_with = "null_to_default")]
    images: Vec<ImageRef>,
}

impl RawEpisode {
    fn into_media_item(self) -> MediaItem {
        let show = self
            .show
            .map(|show| show.name)
            .unwrap_or_else(|| "Podcast episode".to_string());
        MediaItem {
            id: self.id,
            uri: self.uri,
            name: self.name,
            subtitle: show.clone(),
            context: show,
            duration_ms: self.duration_ms,
            image_url: image_url(&self.images),
            kind: MediaKind::Episode,
            source: Some("spotify".to_string()),
            freshness: None,
            explicit: None,
            is_playable: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawAlbum {
    id: Option<String>,
    uri: String,
    name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    artists: Vec<SimpleNamed>,
    #[serde(default, deserialize_with = "null_to_default")]
    images: Vec<ImageRef>,
    total_tracks: Option<u64>,
}

impl RawAlbum {
    fn into_media_item(self) -> MediaItem {
        let artists = join_names(&self.artists);
        MediaItem {
            id: self.id,
            uri: self.uri,
            name: self.name,
            subtitle: artists,
            context: self
                .total_tracks
                .map(|n| format!("{n} tracks"))
                .unwrap_or_default(),
            duration_ms: 0,
            image_url: image_url(&self.images),
            kind: MediaKind::Album,
            source: Some("spotify".to_string()),
            freshness: None,
            explicit: None,
            is_playable: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawArtist {
    id: Option<String>,
    uri: String,
    name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    images: Vec<ImageRef>,
    followers: Option<Followers>,
}

impl RawArtist {
    fn into_media_item(self) -> MediaItem {
        MediaItem {
            id: self.id,
            uri: self.uri,
            name: self.name,
            subtitle: "Artist".to_string(),
            context: self
                .followers
                .map(|followers| format!("{} followers", followers.total))
                .unwrap_or_default(),
            duration_ms: 0,
            image_url: image_url(&self.images),
            kind: MediaKind::Artist,
            source: Some("spotify".to_string()),
            freshness: None,
            explicit: None,
            is_playable: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawPlaylist {
    id: Option<String>,
    uri: Option<String>,
    name: Option<String>,
    owner: Option<PlaylistOwner>,
    tracks: Option<PlaylistTracks>,
    #[serde(default, deserialize_with = "null_to_default")]
    images: Vec<ImageRef>,
    /// Spotify's playlist-version token. Phase 6.5 sync refetch gate
    /// reads this to skip /playlists/{id}/tracks when unchanged.
    #[serde(default)]
    snapshot_id: Option<String>,
}

impl RawPlaylist {
    fn into_playlist(self) -> Option<Playlist> {
        let id = self.id?;
        let tracks_total = self.tracks.as_ref().map(|tracks| tracks.total).unwrap_or(0);
        let snapshot_id = self.snapshot_id.clone();
        Some(Playlist {
            id,
            name: self.name.unwrap_or_else(|| "Untitled playlist".to_string()),
            owner: playlist_owner_name(self.owner),
            tracks_total,
            image_url: image_url(&self.images),
            snapshot_id,
        })
    }

    fn into_media_item(self) -> Option<MediaItem> {
        let id = self.id?;
        let tracks_total = self.tracks.as_ref().map(|tracks| tracks.total).unwrap_or(0);
        Some(MediaItem {
            uri: self.uri.unwrap_or_else(|| format!("spotify:playlist:{id}")),
            id: Some(id),
            name: self.name.unwrap_or_else(|| "Untitled playlist".to_string()),
            subtitle: playlist_owner_name(self.owner),
            context: format!("{tracks_total} tracks"),
            duration_ms: 0,
            image_url: image_url(&self.images),
            kind: MediaKind::Playlist,
            source: Some("spotify".to_string()),
            freshness: None,
            explicit: None,
            is_playable: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SimpleNamed {
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct SimpleShow {
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct PlaylistOwner {
    id: Option<String>,
    display_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PlaylistTracks {
    total: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct Followers {
    total: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct ImageRef {
    url: Option<String>,
    width: Option<u32>,
}

/// Spotify returns `{ "snapshot_id": "..." }` on playlist mutations
/// (add/remove/reorder/replace). The new snapshot is the concurrency
/// token for the next mutation — the daemon persists it so the next
/// undo can compare against it.
#[derive(Debug, Deserialize)]
struct SnapshotResponse {
    snapshot_id: String,
}

// --- Phase 12 (P12-A) URL/body helpers (pure, unit-testable) ---

/// Build the JSON body for `DELETE /playlists/{id}/tracks`.
/// Spotify expects `{ "tracks": [{ "uri": "..." }, ...], "snapshot_id"? }`.
fn playlist_remove_items_body(uris: &[String], snapshot_id: Option<&str>) -> serde_json::Value {
    let tracks: Vec<serde_json::Value> = uris
        .iter()
        .map(|uri| serde_json::json!({ "uri": uri }))
        .collect();
    match snapshot_id {
        Some(snap) => serde_json::json!({ "tracks": tracks, "snapshot_id": snap }),
        None => serde_json::json!({ "tracks": tracks }),
    }
}

/// Build the JSON body for `PUT /playlists/{id}/tracks` reorder.
fn playlist_reorder_body(
    range_start: u32,
    insert_before: u32,
    range_length: u32,
    snapshot_id: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "range_start": range_start,
        "range_length": range_length,
        "insert_before": insert_before,
    });
    if let Some(snap) = snapshot_id {
        body["snapshot_id"] = serde_json::Value::String(snap.to_string());
    }
    body
}

/// Group `(uri, position)` items into `BTreeMap<position, Vec<uri>>`
/// so re-adds use the fewest possible API calls. BTreeMap keeps
/// positions sorted; smallest position first means later inserts
/// don't shift earlier ones.
fn group_items_by_position(items: &[(String, u32)]) -> std::collections::BTreeMap<u32, Vec<String>> {
    let mut grouped: std::collections::BTreeMap<u32, Vec<String>> =
        std::collections::BTreeMap::new();
    for (uri, position) in items {
        grouped.entry(*position).or_default().push(uri.clone());
    }
    grouped
}

/// Resolve a Spotify URI to its library endpoint path and id.
/// Returns `("/me/tracks?ids=abc", "abc")` etc.
fn library_endpoint_for_uri(uri: &str) -> Result<(String, String)> {
    let id = uri
        .rsplit(':')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("malformed Spotify URI `{uri}`"))?
        .to_string();
    let path = match crate::selection::media_kind_from_uri(uri)? {
        MediaKind::Track => format!("/me/tracks?ids={id}"),
        MediaKind::Album => format!("/me/albums?ids={id}"),
        MediaKind::Episode => format!("/me/episodes?ids={id}"),
        MediaKind::Artist => format!("/me/following?type=artist&ids={id}"),
        MediaKind::Playlist => bail!(
            "playlists are saved/unsaved via /playlists/{{id}}/followers, \
             not /me/{{tracks,albums,episodes,artists}}"
        ),
    };
    Ok((path, id))
}

fn playlist_owner_name(owner: Option<PlaylistOwner>) -> String {
    owner
        .and_then(|owner| owner.display_name.or(owner.id))
        .unwrap_or_else(|| "Unknown owner".to_string())
}

fn null_to_default<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

fn join_names(items: &[SimpleNamed]) -> String {
    items
        .iter()
        .map(|item| item.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn image_url(images: &[ImageRef]) -> Option<String> {
    images
        .iter()
        .filter(|image| image.url.is_some())
        .min_by_key(|image| image.width.unwrap_or(u32::MAX).abs_diff(300))
        .and_then(|image| image.url.clone())
}

fn fake_playback() -> Playback {
    Playback {
        item: Some(fake_track()),
        device: Some(fake_device()),
        is_playing: true,
        progress_ms: 42_000,
        shuffle: false,
        repeat: "off".to_string(),
    }
}

fn fake_device() -> Device {
    Device {
        id: Some("fake-device".to_string()),
        name: "spotuify-fake".to_string(),
        kind: "Computer".to_string(),
        is_active: true,
        is_restricted: false,
        volume_percent: Some(70),
        supports_volume: true,
    }
}

fn fake_search_results(query: &str, kinds: &[MediaKind], limit: u8) -> Vec<MediaItem> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    fake_catalog()
        .into_iter()
        .filter(|item| kinds.iter().any(|kind| kind == &item.kind))
        .filter(|item| fake_matches_query(item, query))
        .take(limit as usize)
        .collect()
}

fn fake_matches_query(item: &MediaItem, query: &str) -> bool {
    let haystack = format!("{} {} {}", item.name, item.subtitle, item.context).to_ascii_lowercase();
    query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .all(|token| haystack.contains(&token))
}

fn fake_catalog() -> Vec<MediaItem> {
    vec![
        fake_track(),
        fake_second_track(),
        fake_album(),
        fake_artist(),
        fake_playlist_media_item(),
    ]
}

fn fake_track() -> MediaItem {
    MediaItem {
        id: Some("never-too-much".to_string()),
        uri: "spotify:track:never-too-much".to_string(),
        name: "Never Too Much".to_string(),
        subtitle: "Luther Vandross".to_string(),
        context: "Never Too Much".to_string(),
        duration_ms: 221_000,
        image_url: None,
        kind: MediaKind::Track,
        source: Some("fake".to_string()),
        freshness: None,
        explicit: Some(false),
        is_playable: Some(true),
    }
}

fn fake_second_track() -> MediaItem {
    MediaItem {
        id: Some("sweet-thing".to_string()),
        uri: "spotify:track:sweet-thing".to_string(),
        name: "Sweet Thing".to_string(),
        subtitle: "Chaka Khan".to_string(),
        context: "Rufus featuring Chaka Khan".to_string(),
        duration_ms: 199_000,
        image_url: None,
        kind: MediaKind::Track,
        source: Some("fake".to_string()),
        freshness: None,
        explicit: Some(false),
        is_playable: Some(true),
    }
}

fn fake_album() -> MediaItem {
    MediaItem {
        id: Some("never-too-much-album".to_string()),
        uri: "spotify:album:never-too-much-album".to_string(),
        name: "Never Too Much".to_string(),
        subtitle: "Luther Vandross".to_string(),
        context: "7 tracks".to_string(),
        duration_ms: 0,
        image_url: None,
        kind: MediaKind::Album,
        source: Some("fake".to_string()),
        freshness: None,
        explicit: None,
        is_playable: None,
    }
}

fn fake_artist() -> MediaItem {
    MediaItem {
        id: Some("luther-vandross".to_string()),
        uri: "spotify:artist:luther-vandross".to_string(),
        name: "Luther Vandross".to_string(),
        subtitle: "Artist".to_string(),
        context: "1000000 followers".to_string(),
        duration_ms: 0,
        image_url: None,
        kind: MediaKind::Artist,
        source: Some("fake".to_string()),
        freshness: None,
        explicit: None,
        is_playable: None,
    }
}

fn fake_playlist_media_item() -> MediaItem {
    MediaItem {
        id: Some("quiet-storm".to_string()),
        uri: "spotify:playlist:quiet-storm".to_string(),
        name: "Quiet Storm".to_string(),
        subtitle: "Fake User".to_string(),
        context: "2 tracks".to_string(),
        duration_ms: 0,
        image_url: None,
        kind: MediaKind::Playlist,
        source: Some("fake".to_string()),
        freshness: None,
        explicit: None,
        is_playable: None,
    }
}

fn fake_playlists() -> Vec<Playlist> {
    vec![Playlist {
        id: "quiet-storm".to_string(),
        name: "Quiet Storm".to_string(),
        owner: "Fake User".to_string(),
        tracks_total: 2,
        image_url: None,
        snapshot_id: Some("fake-snap-1".to_string()),
    }]
}

fn fake_playlist_id(name: &str) -> String {
    name.to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn selection_like_uri_check(uri: &str) -> Result<()> {
    if uri.starts_with("spotify:track:")
        || uri.starts_with("spotify:episode:")
        || uri.starts_with("spotify:album:")
        || uri.starts_with("spotify:artist:")
        || uri.starts_with("spotify:playlist:")
    {
        Ok(())
    } else {
        bail!("unsupported Spotify URI `{uri}`")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        group_items_by_position, library_endpoint_for_uri, playlist_remove_items_body,
        playlist_reorder_body, search_path, MediaKind,
    };

    #[test]
    fn search_path_uses_valid_spotify_type_and_limit_params() {
        assert_eq!(
            search_path("luther vandross", &[MediaKind::Track], 10),
            "/search?q=luther+vandross&type=track&limit=10"
        );
    }

    #[test]
    fn search_path_clamps_to_spotify_documented_max_limit() {
        assert_eq!(
            search_path(
                "jazz",
                &[
                    MediaKind::Track,
                    MediaKind::Episode,
                    MediaKind::Album,
                    MediaKind::Artist,
                    MediaKind::Playlist,
                ],
                50,
            ),
            "/search?q=jazz&type=track,episode,album,artist,playlist&limit=10"
        );
    }

    // --- Phase 12 (P12-A) inverse mutator shape tests ---

    #[test]
    fn playlist_remove_items_body_emits_tracks_array_with_uri_field_per_spotify_api() {
        let uris = vec![
            "spotify:track:1".to_string(),
            "spotify:track:2".to_string(),
        ];
        let body = playlist_remove_items_body(&uris, None);
        let tracks = body["tracks"]
            .as_array()
            .expect("body must contain a tracks array");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0]["uri"].as_str().unwrap(), "spotify:track:1");
        assert_eq!(tracks[1]["uri"].as_str().unwrap(), "spotify:track:2");
        // snapshot_id is absent when not provided; presence forces
        // Spotify's optimistic-concurrency precondition which we only
        // want when the daemon captured one.
        assert!(body.get("snapshot_id").is_none());
    }

    #[test]
    fn playlist_remove_items_body_includes_snapshot_id_when_present() {
        let body = playlist_remove_items_body(&["spotify:track:x".to_string()], Some("snap-A"));
        assert_eq!(body["snapshot_id"].as_str().unwrap(), "snap-A");
    }

    #[test]
    fn playlist_reorder_body_carries_all_three_position_fields_and_snapshot() {
        let body = playlist_reorder_body(2, 0, 1, Some("snap-Z"));
        assert_eq!(body["range_start"].as_u64().unwrap(), 2);
        assert_eq!(body["range_length"].as_u64().unwrap(), 1);
        assert_eq!(body["insert_before"].as_u64().unwrap(), 0);
        assert_eq!(body["snapshot_id"].as_str().unwrap(), "snap-Z");
    }

    #[test]
    fn playlist_reorder_body_omits_snapshot_when_unknown() {
        // Spotify rejects requests where snapshot_id is the literal
        // empty string, so we must omit the field entirely when None.
        let body = playlist_reorder_body(0, 5, 3, None);
        assert!(body.get("snapshot_id").is_none());
    }

    #[test]
    fn group_items_by_position_collapses_repeats_and_orders_ascending() {
        let items = vec![
            ("spotify:track:a".to_string(), 3),
            ("spotify:track:b".to_string(), 0),
            ("spotify:track:c".to_string(), 3),
        ];
        let grouped = group_items_by_position(&items);
        let positions: Vec<u32> = grouped.keys().copied().collect();
        // BTreeMap ordering means we process the lowest-position
        // bucket first; that prevents later inserts from shifting
        // earlier indices in the playlist.
        assert_eq!(positions, vec![0, 3]);
        assert_eq!(grouped[&0], vec!["spotify:track:b".to_string()]);
        assert_eq!(
            grouped[&3],
            vec![
                "spotify:track:a".to_string(),
                "spotify:track:c".to_string()
            ]
        );
    }

    #[test]
    fn library_endpoint_for_uri_routes_each_media_kind_to_correct_spotify_endpoint() {
        let cases = [
            ("spotify:track:abc", "/me/tracks?ids=abc"),
            ("spotify:album:xyz", "/me/albums?ids=xyz"),
            ("spotify:episode:e1", "/me/episodes?ids=e1"),
            (
                "spotify:artist:a1",
                "/me/following?type=artist&ids=a1",
            ),
        ];
        for (uri, expected_path) in cases {
            let (path, _id) = library_endpoint_for_uri(uri).unwrap();
            assert_eq!(path, expected_path, "wrong endpoint for {uri}");
        }
    }

    #[test]
    fn user_agent_string_carries_version_os_arch_and_github_url() {
        // Operators triaging Spotify API logs need at least the
        // version, OS, and arch fields to be present and machine-
        // parseable. The GitHub URL is etiquette for third-party
        // services like LRCLIB.
        let ua = super::user_agent_string();
        assert!(ua.starts_with(&format!("spotuify/{}", env!("CARGO_PKG_VERSION"))));
        assert!(ua.contains(std::env::consts::OS));
        assert!(ua.contains(std::env::consts::ARCH));
        assert!(ua.contains("https://github.com/planetaryescape/spotuify"));
    }

    #[test]
    fn library_endpoint_for_uri_rejects_playlists() {
        // Playlists are followed/unfollowed via /playlists/{id}/followers,
        // not the generic /me/* family. Calling library_save on a
        // playlist URI by accident would silently 404; we'd rather
        // bail with a clear error.
        let err = library_endpoint_for_uri("spotify:playlist:p1").unwrap_err();
        assert!(
            err.to_string().contains("playlists"),
            "expected playlist-specific error, got `{err}`"
        );
    }
}
