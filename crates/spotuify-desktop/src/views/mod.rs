use crate::{
    icons::{icon as app_icon, tooltip as icon_tooltip, AppIcon},
    theme::{self, ActiveTheme},
};
use gpui::prelude::*;
use gpui::{
    div, fill, img, point, px, relative, rgb, App, Bounds, ClickEvent, Context, CursorStyle,
    DragMoveEvent, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, Image, ImageFormat, IntoElement, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render,
    ScrollHandle, ShapedLine, SharedString, Style, Subscription, TextRun, UTF16Selection,
    WeakEntity, Window,
};
use spotuify_core::{
    active_lyric_line_index, Device, MediaItem, MediaKind, Playback, Playlist, Queue, RepeatMode,
    SyncedLyrics,
};
use spotuify_launcher::SocketState;
use spotuify_protocol::{
    DaemonEvent, DaemonStatus, DoctorReport, PlaybackCommand, ReceiptId, Request, ResponseData,
    SearchScopeData, SearchSourceData, UpgradeHint,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
#[cfg(debug_assertions)]
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc::UnboundedSender, watch};

pub struct DesktopApp {
    pub(crate) state: DesktopState,
    pub(crate) selected_destination: Destination,
    pub(crate) playback: Option<Playback>,
    pub(crate) queue: Option<Queue>,
    pub(crate) queue_loading: bool,
    pub(crate) queue_requested: bool,
    pub(crate) queue_visible: bool,
    pub(crate) devices: Vec<Device>,
    pub(crate) devices_loading: bool,
    pub(crate) devices_loaded: bool,
    pub(crate) lyrics: Option<SyncedLyrics>,
    pub(crate) lyrics_track_uri: Option<String>,
    pub(crate) lyrics_loading: bool,
    pub(crate) lyrics_error: Option<String>,
    pub(crate) lyrics_offset_ms: i64,
    lyrics_requested_uri: Option<String>,
    pub(crate) update_banner: Option<UpdateBanner>,
    pub(crate) toast: Option<String>,
    pub(crate) command_tx: Option<UnboundedSender<Request>>,
    pub(crate) slider_tx: Option<watch::Sender<Option<Request>>>,
    search_tx: Option<watch::Sender<Option<SearchRequest>>>,
    search_query: String,
    search_results: Vec<MediaItem>,
    pub(crate) search_loading: bool,
    pub(crate) search_error: Option<String>,
    search_version: u64,
    pub(crate) liked_songs: Vec<MediaItem>,
    saved_track_uris: HashSet<String>,
    library_membership_known_uris: HashSet<String>,
    library_membership_requested_uris: HashSet<String>,
    pub(crate) liked_total: u32,
    pub(crate) liked_offset: u32,
    pub(crate) liked_loading: bool,
    pub(crate) liked_error: Option<String>,
    liked_requested: bool,
    pub(crate) albums: Vec<MediaItem>,
    pub(crate) albums_loading: bool,
    pub(crate) albums_error: Option<String>,
    albums_requested: bool,
    pub(crate) artists: Vec<MediaItem>,
    pub(crate) artists_loading: bool,
    pub(crate) artists_error: Option<String>,
    artists_requested: bool,
    pub(crate) history: Vec<MediaItem>,
    pub(crate) history_loading: bool,
    pub(crate) history_error: Option<String>,
    history_requested: bool,
    pub(crate) playlists: Vec<Playlist>,
    pub(crate) playlists_loading: bool,
    pub(crate) playlists_error: Option<String>,
    playlists_requested: bool,
    selected_playlist: Option<Playlist>,
    playlist_tracks: Vec<MediaItem>,
    playlist_tracks_loading: bool,
    playlist_tracks_error: Option<String>,
    selected_album: Option<MediaItem>,
    album_tracks: Vec<MediaItem>,
    album_tracks_loading: bool,
    album_tracks_error: Option<String>,
    selected_artist: Option<MediaItem>,
    detail_history: Vec<DetailLocation>,
    artist_albums: Vec<MediaItem>,
    artist_albums_loading: bool,
    artist_albums_error: Option<String>,
    search_playlists: Vec<Playlist>,
    playlist_picker_uri: Option<String>,
    pub(crate) playlist_loading: bool,
    search_input: Option<Entity<SearchInput>>,
    search_scroll: ScrollHandle,
    liked_songs_scroll: ScrollHandle,
    queue_scroll: ScrollHandle,
    artwork_cache: HashMap<String, Arc<Image>>,
    artwork_requested_urls: HashSet<String>,
    seek_bar_bounds: Option<Bounds<Pixels>>,
    volume_bar_bounds: Option<Bounds<Pixels>>,
    slider_drag: Option<SliderKind>,
    slider_preview: Option<SliderPreview>,
    slider_pending_receipt: Option<ReceiptId>,
    appearance_subscription: Option<Subscription>,
    #[cfg(debug_assertions)]
    debug_fps: DebugFps,
}

#[cfg(debug_assertions)]
struct DebugFps {
    sample_started_at: Instant,
    sampled_frames: u32,
    frames_per_second: f32,
    frame_time_ms: f32,
}

#[cfg(debug_assertions)]
impl DebugFps {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            sample_started_at: now,
            sampled_frames: 0,
            frames_per_second: 0.,
            frame_time_ms: 0.,
        }
    }

    fn record_frame(&mut self, now: Instant) {
        self.sampled_frames = self.sampled_frames.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.sample_started_at);
        if elapsed.as_millis() < 500 {
            return;
        }

        self.frames_per_second = self.sampled_frames as f32 / elapsed.as_secs_f32();
        self.frame_time_ms = 1_000. / self.frames_per_second.max(0.01);
        self.sample_started_at = now;
        self.sampled_frames = 0;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchRequest {
    pub(crate) query: String,
    pub(crate) version: u64,
}

pub(crate) enum DesktopState {
    Booting,
    Gate(GateState),
    Connected(ConnectedState),
}

pub(crate) struct GateState {
    pub(crate) message: String,
    pub(crate) daemon_status: DaemonStatus,
    pub(crate) socket_state: SocketState,
}

pub(crate) struct ConnectedState {
    pub(crate) daemon_status: DaemonStatus,
    pub(crate) doctor_report: Option<Box<DoctorReport>>,
    pub(crate) last_event: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateBanner {
    pub(crate) latest_version: String,
    pub(crate) release_url: Option<String>,
    pub(crate) command: Option<String>,
}

impl UpdateBanner {
    pub(crate) fn new(
        latest_version: String,
        release_url: Option<String>,
        upgrade: UpgradeHint,
    ) -> Self {
        let UpgradeHint { command, url, .. } = upgrade;

        Self {
            latest_version,
            release_url: release_url.or(url),
            command,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Destination {
    NowPlaying,
    Search,
    LikedSongs,
    Albums,
    Artists,
    Podcasts,
    Playlists,
    History,
    Notifications,
    Devices,
    Lyrics,
}

impl Destination {
    const ALL: [Self; 11] = [
        Self::NowPlaying,
        Self::Search,
        Self::LikedSongs,
        Self::Albums,
        Self::Artists,
        Self::Podcasts,
        Self::Playlists,
        Self::History,
        Self::Notifications,
        Self::Devices,
        Self::Lyrics,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::NowPlaying => "now-playing",
            Self::Search => "search",
            Self::LikedSongs => "liked-songs",
            Self::Albums => "albums",
            Self::Artists => "artists",
            Self::Podcasts => "podcasts",
            Self::Playlists => "playlists",
            Self::History => "history",
            Self::Notifications => "notifications",
            Self::Devices => "devices",
            Self::Lyrics => "lyrics",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::NowPlaying => "Now Playing",
            Self::Search => "Search",
            Self::LikedSongs => "Liked Songs",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Podcasts => "Podcasts",
            Self::Playlists => "Playlists",
            Self::History => "History",
            Self::Notifications => "Notifications",
            Self::Devices => "Devices",
            Self::Lyrics => "Lyrics",
        }
    }

    fn icon(self) -> AppIcon {
        match self {
            Self::NowPlaying => AppIcon::NowPlaying,
            Self::Search => AppIcon::Search,
            Self::LikedSongs => AppIcon::LikedSongs,
            Self::Albums => AppIcon::Albums,
            Self::Artists => AppIcon::Artists,
            Self::Podcasts => AppIcon::Podcasts,
            Self::Playlists => AppIcon::Playlists,
            Self::History => AppIcon::History,
            Self::Notifications => AppIcon::Notifications,
            Self::Devices => AppIcon::Devices,
            Self::Lyrics => AppIcon::Lyrics,
        }
    }

    fn stub(self) -> &'static str {
        match self {
            Self::NowPlaying => "Current playback details will expand here.",
            Self::Search => "Search tracks, artists, albums, playlists, and episodes.",
            Self::LikedSongs => "Liked songs will reuse the saved tracks daemon request.",
            Self::Albums => "Saved albums will land with the library panes.",
            Self::Artists => "Followed artists and discography browsing will appear here.",
            Self::Podcasts => "Podcast feeds will reuse the daemon episode feed.",
            Self::Playlists => "Playlist browsing gets wired after the shell.",
            Self::History => "Listening sessions and recent playback will appear here.",
            Self::Notifications => "Reminder and notification inbox state will appear here.",
            Self::Devices => "Device selection will bind to daemon devices state.",
            Self::Lyrics => "Synced lyrics for the current track.",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum DetailLocation {
    Destination(Destination),
    Album(MediaItem),
    Artist(MediaItem),
}

impl DesktopApp {
    pub fn new() -> Self {
        Self {
            state: DesktopState::Booting,
            selected_destination: Destination::NowPlaying,
            playback: None,
            queue: None,
            queue_loading: false,
            queue_requested: false,
            queue_visible: false,
            devices: Vec::new(),
            devices_loading: false,
            devices_loaded: false,
            lyrics: None,
            lyrics_track_uri: None,
            lyrics_loading: false,
            lyrics_error: None,
            lyrics_offset_ms: 0,
            lyrics_requested_uri: None,
            update_banner: None,
            toast: None,
            command_tx: None,
            slider_tx: None,
            search_tx: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_error: None,
            search_version: 0,
            liked_songs: Vec::new(),
            saved_track_uris: HashSet::new(),
            library_membership_known_uris: HashSet::new(),
            library_membership_requested_uris: HashSet::new(),
            liked_total: 0,
            liked_offset: 0,
            liked_loading: false,
            liked_error: None,
            liked_requested: false,
            albums: Vec::new(),
            albums_loading: false,
            albums_error: None,
            albums_requested: false,
            artists: Vec::new(),
            artists_loading: false,
            artists_error: None,
            artists_requested: false,
            history: Vec::new(),
            history_loading: false,
            history_error: None,
            history_requested: false,
            playlists: Vec::new(),
            playlists_loading: false,
            playlists_error: None,
            playlists_requested: false,
            selected_playlist: None,
            playlist_tracks: Vec::new(),
            playlist_tracks_loading: false,
            playlist_tracks_error: None,
            selected_album: None,
            album_tracks: Vec::new(),
            album_tracks_loading: false,
            album_tracks_error: None,
            selected_artist: None,
            detail_history: Vec::new(),
            artist_albums: Vec::new(),
            artist_albums_loading: false,
            artist_albums_error: None,
            search_playlists: Vec::new(),
            playlist_picker_uri: None,
            playlist_loading: false,
            search_input: None,
            search_scroll: ScrollHandle::new(),
            liked_songs_scroll: ScrollHandle::new(),
            queue_scroll: ScrollHandle::new(),
            artwork_cache: HashMap::new(),
            artwork_requested_urls: HashSet::new(),
            seek_bar_bounds: None,
            volume_bar_bounds: None,
            slider_drag: None,
            slider_preview: None,
            slider_pending_receipt: None,
            appearance_subscription: None,
            #[cfg(debug_assertions)]
            debug_fps: DebugFps::new(),
        }
    }

    pub(crate) fn set_command_senders(
        &mut self,
        command_tx: UnboundedSender<Request>,
        slider_tx: watch::Sender<Option<Request>>,
    ) {
        self.command_tx = Some(command_tx);
        self.slider_tx = Some(slider_tx);
    }

    pub(crate) fn set_search_sender(&mut self, search_tx: watch::Sender<Option<SearchRequest>>) {
        self.search_tx = Some(search_tx);
    }

    pub(crate) fn set_queue_seed(&mut self, queue: Queue) {
        self.queue = Some(queue);
        self.queue_loading = false;
        self.queue_requested = true;
        self.request_artwork_for_queue();
    }

    pub(crate) fn set_devices_seed(&mut self, devices: Vec<Device>) {
        self.devices = devices;
        self.devices_loading = false;
        self.devices_loaded = true;
    }

    fn toggle_queue_rail(&mut self) {
        self.queue_visible = !self.queue_visible;
        if self.queue_visible && !self.queue_requested {
            self.queue_loading = true;
            self.queue_requested = true;
            self.send_request(Request::QueueGet);
        }
    }

    fn select_destination(&mut self, destination: Destination) {
        self.detail_history.clear();
        self.close_album();
        self.close_artist();
        self.selected_destination = destination;
        if self.selected_destination == Destination::NowPlaying {
            self.request_artwork_for_current_track();
        }
        if destination == Destination::LikedSongs {
            self.request_liked_songs();
        }
        if destination == Destination::Albums {
            self.request_albums();
        }
        if destination == Destination::Artists {
            self.request_artists();
        }
        if destination == Destination::History {
            self.request_history();
        }
        if destination == Destination::Devices {
            self.request_devices();
        }
        if destination == Destination::Lyrics {
            self.request_lyrics_for_current_track();
        }
        if destination == Destination::Playlists {
            self.request_playlists();
        }
    }

    fn request_playlists(&mut self) {
        if self.playlists_requested || self.playlists_loading || self.command_tx.is_none() {
            return;
        }
        self.playlists_requested = true;
        self.playlists_loading = true;
        self.playlists_error = None;
        self.send_request(Request::PlaylistsList { provider: None });
    }

    fn refresh_playlists(&mut self) {
        if self.command_tx.is_none() || self.playlists_loading {
            return;
        }
        self.playlists_requested = true;
        self.playlists_loading = true;
        self.playlists_error = None;
        self.send_request(Request::PlaylistsList { provider: None });
    }

    pub(crate) fn fail_playlists(&mut self, message: String) {
        self.playlists_loading = false;
        self.playlists_requested = false;
        self.playlist_loading = false;
        self.playlists_error = Some(message);
    }

    fn open_playlist(&mut self, playlist: Playlist) {
        self.selected_playlist = Some(playlist.clone());
        self.playlist_tracks.clear();
        self.playlist_tracks_error = None;
        self.playlist_tracks_loading = true;
        self.send_request(Request::PlaylistTracks {
            playlist: playlist.id,
            wait: false,
            provider: None,
        });
    }

    fn close_playlist(&mut self) {
        self.selected_playlist = None;
        self.playlist_tracks.clear();
        self.playlist_tracks_error = None;
        self.playlist_tracks_loading = false;
    }

    fn current_detail_location(&self) -> DetailLocation {
        match self.selected_destination {
            Destination::Albums => self
                .selected_album
                .clone()
                .map(DetailLocation::Album)
                .unwrap_or(DetailLocation::Destination(Destination::Albums)),
            Destination::Artists => self
                .selected_artist
                .clone()
                .map(DetailLocation::Artist)
                .unwrap_or(DetailLocation::Destination(Destination::Artists)),
            destination => DetailLocation::Destination(destination),
        }
    }

    fn navigate_to_album(&mut self, album: MediaItem) {
        if album.uri.is_empty() {
            return;
        }
        let current = self.current_detail_location();
        if current != DetailLocation::Album(album.clone()) {
            self.detail_history.push(current);
        }
        self.selected_destination = Destination::Albums;
        self.open_album(album);
    }

    fn navigate_to_artist(&mut self, artist: MediaItem) {
        if artist.uri.is_empty() {
            return;
        }
        let current = self.current_detail_location();
        if current != DetailLocation::Artist(artist.clone()) {
            self.detail_history.push(current);
        }
        self.selected_destination = Destination::Artists;
        self.open_artist(artist);
    }

    fn navigate_back_from_detail(&mut self) {
        match self.selected_destination {
            Destination::Albums => self.close_album(),
            Destination::Artists => self.close_artist(),
            _ => {}
        }
        let Some(location) = self.detail_history.pop() else {
            return;
        };
        match location {
            DetailLocation::Destination(destination) => self.selected_destination = destination,
            DetailLocation::Album(album) => {
                self.selected_destination = Destination::Albums;
                self.selected_album = Some(album);
            }
            DetailLocation::Artist(artist) => {
                self.selected_destination = Destination::Artists;
                self.selected_artist = Some(artist);
            }
        }
    }

    fn open_album(&mut self, album: MediaItem) {
        if album.uri.is_empty() {
            return;
        }
        if self
            .selected_album
            .as_ref()
            .is_some_and(|selected| selected.uri == album.uri)
        {
            return;
        }
        self.selected_album = Some(album);
        self.album_tracks.clear();
        self.album_tracks_error = None;
        self.album_tracks_loading = true;
        self.send_request(Request::AlbumTracks {
            album: self
                .selected_album
                .as_ref()
                .expect("album was set")
                .uri
                .clone(),
        });
    }

    fn close_album(&mut self) {
        self.selected_album = None;
        self.album_tracks.clear();
        self.album_tracks_error = None;
        self.album_tracks_loading = false;
    }

    pub(crate) fn fail_album_tracks(&mut self, album: &str, message: String) {
        if self
            .selected_album
            .as_ref()
            .is_some_and(|selected| selected.uri == album)
        {
            self.album_tracks_loading = false;
            self.album_tracks_error = Some(message);
        }
    }

    fn open_artist(&mut self, artist: MediaItem) {
        if artist.uri.is_empty() {
            return;
        }
        if self
            .selected_artist
            .as_ref()
            .is_some_and(|selected| selected.uri == artist.uri)
        {
            return;
        }
        self.selected_artist = Some(artist);
        self.artist_albums.clear();
        self.artist_albums_error = None;
        self.artist_albums_loading = true;
        self.send_request(Request::ArtistAlbums {
            artist: self
                .selected_artist
                .as_ref()
                .expect("artist was set")
                .uri
                .clone(),
        });
    }

    fn close_artist(&mut self) {
        self.selected_artist = None;
        self.artist_albums.clear();
        self.artist_albums_error = None;
        self.artist_albums_loading = false;
    }

    pub(crate) fn fail_artist_albums(&mut self, artist: &str, message: String) {
        if self
            .selected_artist
            .as_ref()
            .is_some_and(|selected| selected.uri == artist)
        {
            self.artist_albums_loading = false;
            self.artist_albums_error = Some(message);
        }
    }

    pub(crate) fn fail_playlist_tracks(&mut self, playlist: &str, message: String) {
        if self
            .selected_playlist
            .as_ref()
            .is_some_and(|item| item.id == playlist)
        {
            self.playlist_tracks_loading = false;
            self.playlist_tracks_error = Some(message);
        }
    }

    pub(crate) fn request_devices(&mut self) {
        if self.devices_loaded || self.devices_loading {
            return;
        }
        if self.command_tx.is_none() {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        }
        self.devices_loading = true;
        self.send_request(Request::DevicesList);
    }

    fn request_liked_songs(&mut self) {
        if self.liked_requested || self.command_tx.is_none() {
            return;
        }
        self.liked_requested = true;
        self.liked_loading = true;
        self.liked_error = None;
        self.send_request(Request::SavedTracks {
            limit: 50,
            offset: 0,
            provider: None,
        });
    }

    fn refresh_liked_songs(&mut self) {
        if self.command_tx.is_none() || self.liked_loading {
            return;
        }
        self.liked_requested = true;
        self.liked_loading = true;
        self.liked_error = None;
        self.send_request(Request::SavedTracks {
            limit: 50,
            offset: 0,
            provider: None,
        });
    }

    fn request_albums(&mut self) {
        if self.albums_requested || self.albums_loading || self.command_tx.is_none() {
            return;
        }
        self.albums_requested = true;
        self.albums_loading = true;
        self.albums_error = None;
        self.send_request(Request::LibraryList {
            limit: 100,
            provider: None,
        });
    }

    fn refresh_albums(&mut self) {
        if self.command_tx.is_none() || self.albums_loading {
            return;
        }
        self.albums_requested = true;
        self.albums_loading = true;
        self.albums_error = None;
        self.send_request(Request::LibraryList {
            limit: 100,
            provider: None,
        });
    }

    pub(crate) fn fail_albums(&mut self, message: String) {
        self.albums_loading = false;
        self.albums_requested = false;
        self.albums_error = Some(message);
    }

    fn request_artists(&mut self) {
        if self.artists_requested || self.artists_loading || self.command_tx.is_none() {
            return;
        }
        self.artists_requested = true;
        self.artists_loading = true;
        self.artists_error = None;
        self.send_request(Request::FollowedArtists {
            limit: 100,
            provider: None,
        });
    }

    fn refresh_artists(&mut self) {
        if self.command_tx.is_none() || self.artists_loading {
            return;
        }
        self.artists_requested = true;
        self.artists_loading = true;
        self.artists_error = None;
        self.send_request(Request::FollowedArtists {
            limit: 100,
            provider: None,
        });
    }

    pub(crate) fn fail_artists(&mut self, message: String) {
        self.artists_loading = false;
        self.artists_requested = false;
        self.artists_error = Some(message);
    }

    fn request_history(&mut self) {
        if self.history_requested || self.history_loading || self.command_tx.is_none() {
            return;
        }
        self.history_requested = true;
        self.history_loading = true;
        self.history_error = None;
        self.send_request(Request::RecentlyPlayed { provider: None });
    }

    fn refresh_history(&mut self) {
        if self.command_tx.is_none() || self.history_loading {
            return;
        }
        self.history_requested = true;
        self.history_loading = true;
        self.history_error = None;
        self.send_request(Request::RecentlyPlayed { provider: None });
    }

    pub(crate) fn fail_liked_songs(&mut self, message: String) {
        self.liked_loading = false;
        self.liked_error = Some(message);
    }

    pub(crate) fn fail_history(&mut self, message: String) {
        self.history_loading = false;
        self.history_requested = false;
        self.history_error = Some(message);
    }

    fn transfer_to_device(&mut self, device: String) {
        self.send_request(Request::DeviceTransfer { device });
        self.toast = Some("Transferring playback".to_string());
    }

    fn send_request(&mut self, request: Request) {
        let Some(command_tx) = &self.command_tx else {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        };

        if command_tx.send(request).is_err() {
            self.toast = Some("Transport connection closed".to_string());
        }
    }

    fn request_lyrics_for_current_track(&mut self) {
        let Some(uri) = self.current_track_uri() else {
            self.lyrics = None;
            self.lyrics_track_uri = None;
            self.lyrics_loading = false;
            self.lyrics_error = None;
            self.lyrics_requested_uri = None;
            return;
        };
        if self.lyrics_requested_uri.as_deref() == Some(uri.as_str()) {
            return;
        }
        if self.command_tx.is_none() {
            return;
        }
        self.lyrics_requested_uri = Some(uri.clone());
        self.lyrics_track_uri = Some(uri.clone());
        self.lyrics = None;
        self.lyrics_error = None;
        self.lyrics_offset_ms = 0;
        self.lyrics_loading = true;
        self.send_request(Request::LyricsGet {
            track_uri: Some(uri),
            force_refresh: false,
        });
    }

    fn current_track_uri(&self) -> Option<String> {
        self.playback
            .as_ref()
            .and_then(|playback| playback.item.as_ref())
            .map(|item| item.uri.clone())
    }

    fn current_track_like_status(&self) -> Option<bool> {
        let uri = self.current_track_uri()?;
        self.library_membership_known_uris
            .contains(&uri)
            .then(|| self.saved_track_uris.contains(&uri))
    }

    pub(crate) fn request_current_track_membership(&mut self) {
        if self.command_tx.is_none() {
            return;
        }
        let Some(uri) = self.current_track_uri() else {
            return;
        };
        if self.library_membership_requested_uris.insert(uri.clone()) {
            self.send_request(Request::LibraryContains { uris: vec![uri] });
        }
    }

    fn toggle_current_track_like(&mut self) {
        let Some(uri) = self.current_track_uri() else {
            return;
        };
        match self.current_track_like_status() {
            Some(true) => self.send_request(Request::LibraryUnsave { uri }),
            Some(false) => self.send_request(Request::LibrarySave {
                uri: Some(uri),
                current: false,
            }),
            None => {}
        }
    }

    fn current_artwork_url(&self, large: bool) -> Option<String> {
        self.playback
            .as_ref()
            .and_then(|playback| playback.item.as_ref())
            .and_then(|item| artwork_url(item, large))
    }

    pub(crate) fn request_artwork_for_current_track(&mut self) {
        if let Some(url) = self.current_artwork_url(true) {
            self.request_artwork(url);
        }
    }

    fn request_artwork_for_queue(&mut self) {
        let urls: Vec<_> = self
            .queue
            .iter()
            .flat_map(|queue| queue.currently_playing.iter().chain(&queue.items))
            .filter_map(|item| artwork_url(item, false))
            .collect();
        for url in urls {
            self.request_artwork(url);
        }
    }

    fn request_artwork_for_liked_songs(&mut self) {
        let urls: Vec<_> = self
            .liked_songs
            .iter()
            .filter_map(|item| artwork_url(item, false))
            .collect();
        for url in urls {
            self.request_artwork(url);
        }
    }

    fn request_artwork(&mut self, url: String) {
        if self.artwork_cache.contains_key(&url)
            || self.artwork_requested_urls.contains(&url)
            || self.command_tx.is_none()
        {
            return;
        }
        self.artwork_requested_urls.insert(url.clone());
        self.send_request(Request::Image { url });
    }

    pub(crate) fn apply_artwork_response(&mut self, url: String, response: ResponseData) {
        if let ResponseData::Image { bytes } = response {
            if let Some(format) = image_format(&bytes) {
                self.artwork_cache
                    .insert(url.clone(), Arc::new(Image::from_bytes(format, bytes)));
            }
        }
        self.artwork_requested_urls.remove(&url);
    }

    pub(crate) fn fail_artwork(&mut self, url: &str) {
        self.artwork_requested_urls.remove(url);
    }

    pub(crate) fn fail_lyrics(&mut self, track_uri: &str, message: String) {
        if self.lyrics_track_uri.as_deref() == Some(track_uri) {
            self.lyrics_loading = false;
            self.lyrics_error = Some(message);
        }
    }

    fn send_playback_command(&mut self, command: PlaybackCommand) {
        self.send_request(Request::PlaybackCommand { command });
    }

    fn send_slider_command(&mut self, command: PlaybackCommand) {
        let Some(slider_tx) = &self.slider_tx else {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        };

        if slider_tx
            .send(Some(Request::PlaybackCommand { command }))
            .is_err()
        {
            self.toast = Some("Transport connection closed".to_string());
        }
    }

    fn update_search_query(&mut self, query: String) {
        if self.search_query != query {
            self.search_query = query;
            self.search_version = self.search_version.wrapping_add(1);
            self.search_results.clear();
            self.search_error = None;
            self.search_loading = !self.search_query.trim().is_empty();
            if let Some(search_tx) = &self.search_tx {
                let request = (!self.search_query.trim().is_empty()).then(|| SearchRequest {
                    query: self.search_query.trim().to_string(),
                    version: self.search_version,
                });
                let _ = search_tx.send(request);
            }
        }
    }

    fn start_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            self.search_results.clear();
            self.search_error = None;
            self.search_loading = false;
            return;
        }

        if self.search_version == 0 {
            self.search_version = 1;
        }
        self.search_results.clear();
        self.search_error = None;
        self.search_loading = true;
        if let Some(search_tx) = &self.search_tx {
            let _ = search_tx.send(None);
        }
        self.send_request(Request::SearchStream {
            query,
            scope: SearchScopeData::All,
            source: SearchSourceData::legacy_default_remote(),
            version: self.search_version,
            provider: None,
        });
    }

    fn open_playlist_picker(&mut self, uri: String) {
        self.playlist_picker_uri = Some(uri);
        self.playlist_loading = true;
        self.search_playlists.clear();
        self.send_request(Request::PlaylistsList { provider: None });
    }

    fn add_search_result_to_playlist(&mut self, playlist: String) {
        let Some(uri) = self.playlist_picker_uri.take() else {
            return;
        };
        self.playlist_loading = false;
        self.send_request(Request::PlaylistAddItems {
            playlist,
            uris: vec![uri],
            provider: None,
        });
        self.toast = Some("Adding track to playlist".to_string());
    }

    pub(crate) fn apply_daemon_response(&mut self, response: ResponseData) {
        self.apply_daemon_response_for_track(response, None);
    }

    pub(crate) fn apply_albums_response(&mut self, response: ResponseData) {
        if let ResponseData::MediaItems { items } = response {
            self.albums = items
                .into_iter()
                .filter(|item| item.kind == MediaKind::Album)
                .collect();
            self.albums_loading = false;
            self.albums_error = None;
            self.albums_requested = true;
        }
    }

    pub(crate) fn apply_artists_response(&mut self, response: ResponseData) {
        if let ResponseData::MediaItems { items } = response {
            self.artists = items;
            self.artists_loading = false;
            self.artists_error = None;
            self.artists_requested = true;
        }
    }

    pub(crate) fn apply_history_response(&mut self, response: ResponseData) {
        if let ResponseData::MediaItems { items } = response {
            self.history = items;
            self.history_loading = false;
            self.history_error = None;
            self.history_requested = true;
        }
    }

    pub(crate) fn apply_daemon_response_for_track(
        &mut self,
        response: ResponseData,
        request_track_uri: Option<&str>,
    ) {
        match response {
            ResponseData::Playlists { playlists } => {
                self.playlists = playlists.clone();
                self.playlists_error = None;
                self.search_playlists = playlists;
                self.playlist_loading = false;
                self.playlists_loading = false;
                self.playlists_requested = true;
            }
            ResponseData::Queue { queue } => self.set_queue_seed(queue),
            ResponseData::Devices { devices } => {
                self.devices = devices;
                self.devices_loading = false;
                self.devices_loaded = true;
            }
            ResponseData::Lyrics { lyrics, offset_ms } => {
                let Some(request_track_uri) = request_track_uri else {
                    return;
                };
                if self.lyrics_track_uri.as_deref() != Some(request_track_uri)
                    || self.current_track_uri().as_deref() != Some(request_track_uri)
                    || lyrics
                        .as_ref()
                        .is_some_and(|lyrics| lyrics.track_uri != request_track_uri)
                {
                    return;
                }
                self.lyrics = lyrics;
                self.lyrics_offset_ms = offset_ms;
                self.lyrics_loading = false;
                self.lyrics_error = None;
            }
            ResponseData::LyricsOffset {
                track_uri,
                offset_ms,
            } if self.lyrics_track_uri.as_deref() == Some(track_uri.as_str()) => {
                self.lyrics_offset_ms = offset_ms;
            }
            ResponseData::SavedTracksPage {
                items,
                total,
                offset,
            } => {
                self.saved_track_uris
                    .extend(items.iter().map(|item| item.uri.clone()));
                self.library_membership_known_uris
                    .extend(items.iter().map(|item| item.uri.clone()));
                self.liked_songs = items;
                self.request_artwork_for_liked_songs();
                self.liked_total = total;
                self.liked_offset = offset;
                self.liked_loading = false;
                self.liked_error = None;
                self.liked_requested = true;
            }
            ResponseData::LibraryMembership { memberships } => {
                for membership in memberships {
                    self.library_membership_known_uris
                        .insert(membership.uri.clone());
                    if membership.saved {
                        self.saved_track_uris.insert(membership.uri);
                    } else {
                        self.saved_track_uris.remove(&membership.uri);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn apply_playlist_tracks_response(
        &mut self,
        playlist: &str,
        response: ResponseData,
    ) {
        if !self
            .selected_playlist
            .as_ref()
            .is_some_and(|selected| selected.id == playlist)
        {
            return;
        }
        if let ResponseData::MediaItems { items } = response {
            self.playlist_tracks = items;
            self.playlist_tracks_loading = false;
            self.playlist_tracks_error = None;
        }
    }

    pub(crate) fn apply_album_tracks_response(&mut self, album: &str, response: ResponseData) {
        if self
            .selected_album
            .as_ref()
            .is_none_or(|selected| selected.uri != album)
        {
            return;
        }
        if let ResponseData::MediaItems { items } = response {
            self.album_tracks = items;
            self.album_tracks_loading = false;
            self.album_tracks_error = None;
        }
    }

    pub(crate) fn apply_artist_albums_response(&mut self, artist: &str, response: ResponseData) {
        if self
            .selected_artist
            .as_ref()
            .is_none_or(|selected| selected.uri != artist)
        {
            return;
        }
        if let ResponseData::MediaItems { items } = response {
            self.artist_albums = items;
            self.artist_albums_loading = false;
            self.artist_albums_error = None;
        }
    }

    pub(crate) fn fail_search(&mut self, query: &str, version: u64, message: String) {
        if version == self.search_version && query == self.search_query.trim() {
            self.search_loading = false;
            self.search_error = Some(message);
        }
    }

    fn preview_slider(&mut self, kind: SliderKind, preview: SliderPreview) {
        self.slider_drag = Some(kind);
        self.slider_preview = Some(preview);
        self.slider_pending_receipt = None;
    }

    fn finish_slider_drag(&mut self, kind: SliderKind) {
        match (kind, self.slider_preview) {
            (SliderKind::Seek, Some(SliderPreview::Seek(position_ms))) => {
                self.slider_drag = None;
                self.slider_pending_receipt = None;
                self.send_slider_command(PlaybackCommand::Seek { position_ms });
            }
            (SliderKind::Volume, Some(SliderPreview::Volume(volume_percent))) => {
                self.slider_drag = None;
                self.slider_pending_receipt = None;
                self.send_slider_command(PlaybackCommand::Volume { volume_percent });
            }
            _ => {}
        }
    }

    pub(crate) fn apply_daemon_event(&mut self, event: DaemonEvent) {
        let label = event_label(&event);

        if let DesktopState::Connected(state) = &mut self.state {
            state.last_event = Some(label);
        }

        match event {
            DaemonEvent::PlaybackChanged { action, playback } => {
                if let Some(playback) = playback {
                    let settles_slider = self.slider_drag.is_none()
                        && self.slider_preview.is_some_and(|preview| {
                            slider_preview_matches_playback(preview, &playback)
                        });
                    self.playback = Some(playback);
                    self.request_artwork_for_current_track();
                    self.request_current_track_membership();
                    if self.current_track_uri().as_deref() != self.lyrics_track_uri.as_deref() {
                        self.lyrics_requested_uri = None;
                        self.request_lyrics_for_current_track();
                    }
                    if settles_slider {
                        self.slider_preview = None;
                        self.slider_pending_receipt = None;
                    }
                } else {
                    self.request_lyrics_for_current_track();
                }
                if should_toast_playback_action(&action) {
                    self.toast = Some(format!("Playback updated: {action}"));
                }
            }
            DaemonEvent::QueueChanged {
                queue: Some(queue), ..
            } => self.set_queue_seed(queue),
            DaemonEvent::DevicesChanged {
                action, devices, ..
            } => {
                if let Some(devices) = devices {
                    self.set_devices_seed(devices);
                }
                if !matches!(action.as_str(), "snapshot" | "synced" | "sync" | "poll") {
                    self.toast = Some(format!("Devices updated: {action}"));
                }
            }
            DaemonEvent::LibraryChanged { action, uris, .. } => {
                match action.as_str() {
                    "save" => {
                        self.saved_track_uris.extend(uris.iter().cloned());
                        self.library_membership_known_uris
                            .extend(uris.iter().cloned());
                    }
                    "unsave" => {
                        for uri in &uris {
                            self.saved_track_uris.remove(uri);
                            self.library_membership_known_uris.insert(uri.clone());
                        }
                    }
                    _ => {}
                }
                if self.liked_requested {
                    self.refresh_liked_songs();
                }
                if self.albums_requested {
                    self.refresh_albums();
                }
                if self.artists_requested {
                    self.refresh_artists();
                }
            }
            DaemonEvent::PlaylistsChanged { .. } if self.playlists_requested => {
                self.refresh_playlists();
            }
            DaemonEvent::SyncFinished { summary }
                if self.history_requested
                    && matches!(
                        summary.target,
                        spotuify_protocol::SyncTargetData::All
                            | spotuify_protocol::SyncTargetData::Recent
                    ) =>
            {
                self.refresh_history();
            }
            DaemonEvent::SearchPage {
                query,
                version,
                items,
                ..
            } if version == self.search_version && query == self.search_query.trim() => {
                self.search_results.extend(items);
                sort_search_results(&mut self.search_results);
            }
            DaemonEvent::SearchComplete { query, version, .. }
                if version == self.search_version && query == self.search_query.trim() =>
            {
                self.search_loading = false;
            }
            DaemonEvent::SearchFailed {
                query,
                version,
                message,
                ..
            } if version == self.search_version && query == self.search_query.trim() => {
                // SearchStream fans out one request per media kind. A single
                // failed kind must not hide successful track pages that are
                // still in flight; SearchComplete closes the loading state.
                self.search_error = Some(message);
            }
            DaemonEvent::MutationFinalized {
                receipt_id,
                status,
                message,
            } => match status {
                spotuify_protocol::ReceiptStatus::Failed => {
                    if self.slider_pending_receipt == Some(receipt_id) {
                        self.slider_preview = None;
                        self.slider_pending_receipt = None;
                    }
                    self.toast = Some(format!("Mutation failed: {message}"));
                }
                spotuify_protocol::ReceiptStatus::Confirmed => {
                    self.toast = Some(message);
                }
                spotuify_protocol::ReceiptStatus::Pending => {}
            },
            DaemonEvent::MutationAccepted { receipt_id, action } => {
                if self.slider_drag.is_none()
                    && self
                        .slider_preview
                        .is_some_and(|preview| preview.action() == action)
                {
                    self.slider_pending_receipt = Some(receipt_id);
                }
            }
            DaemonEvent::UpdateAvailable {
                latest_version,
                release_url,
                upgrade,
            } => {
                self.update_banner = Some(UpdateBanner::new(
                    latest_version.clone(),
                    release_url,
                    upgrade,
                ));
                self.toast = Some(format!("Update available: {latest_version}"));
            }
            DaemonEvent::MutationFinished { message, .. } => {
                self.toast = Some(message);
            }
            DaemonEvent::EventStreamLagged { skipped } => {
                self.toast = Some(format!("Event stream lagged by {skipped} events"));
            }
            DaemonEvent::AuthError { kind, .. } => {
                self.toast = Some(format!("Authentication needs attention: {kind:?}"));
            }
            DaemonEvent::PlayerDegraded { reason }
            | DaemonEvent::SessionDisconnected { reason }
            | DaemonEvent::PlayerFailed { reason, .. } => {
                self.toast = Some(reason);
            }
            DaemonEvent::PremiumRequired => {
                self.toast = Some("Spotify Premium is required for playback".to_string());
            }
            _ => {}
        }
    }
}

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if self.appearance_subscription.is_none() {
            theme::sync_system_appearance(window.appearance(), cx);
            self.appearance_subscription =
                Some(cx.observe_window_appearance(window, |_, window, cx| {
                    theme::sync_system_appearance(window.appearance(), cx);
                    cx.notify();
                }));
        }
        self.ensure_search_input(cx);

        #[cfg(debug_assertions)]
        {
            self.debug_fps.record_frame(Instant::now());
            window.request_animation_frame();
        }

        if self
            .playback
            .as_ref()
            .is_some_and(|playback| playback.is_playing)
        {
            // Progress is derived from the daemon's sampled timestamp and the
            // local clock. The app never advances authoritative playback state;
            // this frame request only keeps the rendered seek bar moving.
            window.request_animation_frame();
        }
        match &self.state {
            DesktopState::Booting => diagnostics_surface(
                cx,
                "connecting...",
                &crate::platform::macos::placeholder_message(),
                vec![
                    "daemon: checking".to_string(),
                    "socket: checking".to_string(),
                    "auth: checking".to_string(),
                    "version: checking".to_string(),
                ],
            )
            .into_any_element(),
            DesktopState::Gate(state) => diagnostics_surface(
                cx,
                "daemon gate",
                &state.message,
                vec![
                    format!("daemon: {}", health_word(state.daemon_status.running)),
                    format!("socket: {:?}", state.socket_state),
                    format!(
                        "version: {}",
                        state
                            .daemon_status
                            .daemon_version
                            .as_deref()
                            .unwrap_or("unknown")
                    ),
                    format!(
                        "auth: {}",
                        if state.daemon_status.socket_reachable {
                            "available via daemon"
                        } else {
                            "unknown until daemon is reachable"
                        }
                    ),
                ],
            )
            .into_any_element(),
            DesktopState::Connected(state) => self.app_shell(state, cx).into_any_element(),
        }
    }
}

impl DesktopApp {
    fn ensure_search_input(&mut self, cx: &mut Context<'_, Self>) {
        if self.search_input.is_none() {
            let desktop_app = cx.entity().downgrade();
            self.search_input = Some(cx.new(|cx| SearchInput::new(cx, desktop_app)));
        }
    }

    fn app_shell(&self, state: &ConnectedState, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut content = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(cx.desktop_theme().bg_root));

        if let Some(banner) = &self.update_banner {
            content = content.child(update_banner_surface(cx, banner));
        }

        content = if self.selected_destination == Destination::Search {
            content.child(self.search_pane(cx))
        } else if self.selected_destination == Destination::NowPlaying {
            content.child(self.now_playing_pane(cx))
        } else if self.selected_destination == Destination::Devices {
            content.child(self.devices_pane(cx))
        } else if self.selected_destination == Destination::Lyrics {
            content.child(self.lyrics_pane(cx))
        } else if self.selected_destination == Destination::LikedSongs {
            content.child(self.liked_songs_pane(cx))
        } else if self.selected_destination == Destination::Albums {
            content.child(self.albums_pane(cx))
        } else if self.selected_destination == Destination::Artists {
            content.child(self.artists_pane(cx))
        } else if self.selected_destination == Destination::History {
            content.child(self.history_pane(cx))
        } else if self.selected_destination == Destination::Playlists {
            content.child(self.playlists_pane(cx))
        } else {
            content.child(self.content_pane(state, cx))
        }
        .child(self.now_playing_footer(cx));

        let mut root = div()
            .size_full()
            .relative()
            .bg(rgb(cx.desktop_theme().bg_root))
            .text_color(rgb(cx.desktop_theme().text_primary))
            .flex()
            .flex_row()
            .child(self.sidebar(cx))
            .child(content);

        if let Some(toast) = &self.toast {
            root = root.child(toast_surface(cx, toast));
        }

        #[cfg(debug_assertions)]
        {
            root = root.child(debug_fps_surface(cx, &self.debug_fps));
        }

        root
    }

    fn sidebar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut nav = div()
            .w(px(238.))
            .h_full()
            .bg(rgb(cx.desktop_theme().bg_sidebar))
            .border_r_1()
            .border_color(rgb(cx.desktop_theme().border))
            .px_4()
            .py_5()
            .flex()
            .flex_col();

        nav = nav
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(cx.desktop_theme().accent))
                    .child("SPOTUIFY"),
            )
            .child(div().mt_1().mb_6().text_2xl().child("Desktop"));

        for destination in Destination::ALL {
            nav = nav.child(nav_item(
                destination,
                destination == self.selected_destination,
                cx,
            ));
        }

        nav
    }

    fn content_pane(
        &self,
        state: &ConnectedState,
        cx: &mut Context<'_, DesktopApp>,
    ) -> impl IntoElement {
        let auth = state
            .doctor_report
            .as_ref()
            .map(|report| report.keychain_token.message.as_str())
            .unwrap_or("unknown");

        div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .child(div().text_3xl().child(self.selected_destination.label()))
                            .child(
                                div()
                                    .mt_2()
                                    .text_color(rgb(cx.desktop_theme().text_secondary))
                                    .child(self.selected_destination.stub()),
                            ),
                    )
                    .child(
                        div()
                            .w(px(LAST_EVENT_WIDTH))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(rgb(cx.desktop_theme().text_secondary))
                            .child(format!("last event: {}", state.last_event.as_deref().unwrap_or("none"))),
                    ),
            )
            .child(
                div()
                    .mt_8()
                    .border_1()
                    .border_color(rgb(cx.desktop_theme().border))
                    .bg(rgb(cx.desktop_theme().bg_surface))
                    .rounded_lg()
                    .p_6()
                    .child(div().text_lg().child(format!("{} pane", self.selected_destination.label())))
                    .child(
                        div()
                            .mt_3()
                            .text_color(rgb(cx.desktop_theme().text_secondary))
                            .child("This is a routed shell stub. The sidebar already switches panes; data-heavy panes arrive in their own tickets."),
                    )
                    .child(
                        div()
                            .mt_5()
                            .text_sm()
                            .text_color(rgb(cx.desktop_theme().text_muted))
                            .child(format!(
                                "daemon: {} | auth: {} | version: {}",
                                health_word(state.daemon_status.running),
                                auth,
                                state.daemon_status.daemon_version.as_deref().unwrap_or("unknown")
                            )),
                    ),
            )
    }

    fn liked_songs_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Liked Songs"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child(format!("{} songs", self.liked_total)),
            );

        if self.liked_loading && self.liked_songs.is_empty() {
            return pane.child(queue_message(cx, "Loading liked songs..."));
        }
        if let Some(error) = &self.liked_error {
            return pane.child(queue_message(
                cx,
                &format!("Couldn't load liked songs: {error}"),
            ));
        }
        if self.liked_songs.is_empty() {
            return pane.child(queue_message(cx, "No liked songs"));
        }

        let mut rows = div()
            .id("liked-songs")
            .mt_4()
            .h(px(0.))
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.liked_songs_scroll)
            .flex()
            .flex_col()
            .gap_1();
        for (index, item) in self.liked_songs.iter().enumerate() {
            rows = rows.child(liked_song_row(index, item, &self.artwork_cache, cx));
        }
        pane.child(rows)
    }

    fn albums_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        if let Some(album) = &self.selected_album {
            return self.album_detail_pane(album, cx).into_any_element();
        }
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Albums"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Saved albums"),
            );

        if self.albums_loading && self.albums.is_empty() {
            return pane
                .child(queue_message(cx, "Loading albums..."))
                .into_any_element();
        }
        if let Some(error) = &self.albums_error {
            return pane
                .child(queue_message(cx, &format!("Couldn't load albums: {error}")))
                .into_any_element();
        }
        if self.albums.is_empty() {
            return pane
                .child(queue_message(cx, "No saved albums"))
                .into_any_element();
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.albums.iter().enumerate() {
            rows = rows.child(album_row(index, item, cx));
        }
        pane.child(rows).into_any_element()
    }

    fn artists_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        if let Some(artist) = &self.selected_artist {
            return self.artist_detail_pane(artist, cx).into_any_element();
        }
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Artists"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Followed artists"),
            );

        if self.artists_loading && self.artists.is_empty() {
            return pane
                .child(queue_message(cx, "Loading artists..."))
                .into_any_element();
        }
        if let Some(error) = &self.artists_error {
            return pane
                .child(queue_message(
                    cx,
                    &format!("Couldn't load artists: {error}"),
                ))
                .into_any_element();
        }
        if self.artists.is_empty() {
            return pane
                .child(queue_message(cx, "No followed artists"))
                .into_any_element();
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.artists.iter().enumerate() {
            rows = rows.child(artist_row(index, item, cx));
        }
        pane.child(rows).into_any_element()
    }

    fn history_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("History"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Recently played tracks"),
            );

        if self.history_loading && self.history.is_empty() {
            return pane.child(queue_message(cx, "Loading history..."));
        }
        if let Some(error) = &self.history_error {
            return pane.child(queue_message(
                cx,
                &format!("Couldn't load history: {error}"),
            ));
        }
        if self.history.is_empty() {
            return pane.child(queue_message(cx, "No recently played tracks"));
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.history.iter().enumerate() {
            rows = rows.child(history_row(index, item, cx));
        }
        pane.child(rows)
    }

    fn playlists_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        if let Some(playlist) = &self.selected_playlist {
            return self.playlist_detail_pane(playlist, cx).into_any_element();
        }

        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Playlists"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Your playlists"),
            );

        if self.playlists_loading && self.playlists.is_empty() {
            return pane
                .child(queue_message(cx, "Loading playlists..."))
                .into_any_element();
        }
        if let Some(error) = &self.playlists_error {
            return pane
                .child(queue_message(
                    cx,
                    &format!("Couldn't load playlists: {error}"),
                ))
                .into_any_element();
        }
        if self.playlists.is_empty() {
            return pane
                .child(queue_message(cx, "No playlists"))
                .into_any_element();
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, playlist) in self.playlists.iter().enumerate() {
            rows = rows.child(playlist_row(index, playlist, cx));
        }
        pane.child(rows).into_any_element()
    }

    fn album_detail_pane(
        &self,
        album: &MediaItem,
        cx: &mut Context<'_, DesktopApp>,
    ) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(search_row_action(
                        cx,
                        "Back",
                        cx.listener(|app, _, _, cx| {
                            app.navigate_back_from_detail();
                            cx.notify();
                        }),
                    ))
                    .child(div().text_3xl().child(if album.name.is_empty() {
                        "Untitled album".to_string()
                    } else {
                        album.name.clone()
                    })),
            )
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child(media_context_links("album-detail", album, cx)),
            );

        if self.album_tracks_loading && self.album_tracks.is_empty() {
            return pane.child(queue_message(cx, "Loading album tracks..."));
        }
        if let Some(error) = &self.album_tracks_error {
            return pane.child(queue_message(
                cx,
                &format!("Couldn't load album tracks: {error}"),
            ));
        }
        if self.album_tracks.is_empty() {
            return pane.child(queue_message(cx, "No tracks in this album"));
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.album_tracks.iter().enumerate() {
            rows = rows.child(playlist_track_row(index, item, cx));
        }
        pane.child(rows)
    }

    fn artist_detail_pane(
        &self,
        artist: &MediaItem,
        cx: &mut Context<'_, DesktopApp>,
    ) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(search_row_action(
                        cx,
                        "Back",
                        cx.listener(|app, _, _, cx| {
                            app.navigate_back_from_detail();
                            cx.notify();
                        }),
                    ))
                    .child(div().text_3xl().child(if artist.name.is_empty() {
                        "Unnamed artist".to_string()
                    } else {
                        artist.name.clone()
                    })),
            )
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Discography"),
            );

        if self.artist_albums_loading && self.artist_albums.is_empty() {
            return pane.child(queue_message(cx, "Loading artist albums..."));
        }
        if let Some(error) = &self.artist_albums_error {
            return pane.child(queue_message(
                cx,
                &format!("Couldn't load artist albums: {error}"),
            ));
        }
        if self.artist_albums.is_empty() {
            return pane.child(queue_message(cx, "No albums for this artist"));
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.artist_albums.iter().enumerate() {
            rows = rows.child(detail_album_row(index, item, cx));
        }
        pane.child(rows)
    }

    fn playlist_detail_pane(
        &self,
        playlist: &Playlist,
        cx: &mut Context<'_, DesktopApp>,
    ) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(search_row_action(
                        cx,
                        "Back",
                        cx.listener(|app, _, _, cx| {
                            app.close_playlist();
                            cx.notify();
                        }),
                    ))
                    .child(div().text_3xl().child(playlist.name.clone())),
            )
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child(format!(
                        "{} tracks · {}",
                        playlist.tracks_total, playlist.owner
                    )),
            );

        if self.playlist_tracks_loading && self.playlist_tracks.is_empty() {
            return pane.child(queue_message(cx, "Loading playlist tracks..."));
        }
        if let Some(error) = &self.playlist_tracks_error {
            return pane.child(queue_message(
                cx,
                &format!("Couldn't load playlist tracks: {error}"),
            ));
        }
        if self.playlist_tracks.is_empty() {
            return pane.child(queue_message(cx, "No tracks in this playlist"));
        }

        let mut rows = div().mt_6().flex().flex_col().gap_2();
        for (index, item) in self.playlist_tracks.iter().enumerate() {
            rows = rows.child(playlist_track_row(index, item, cx));
        }
        pane.child(rows)
    }

    fn now_playing_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let playback = self.playback.as_ref();
        let item = playback.and_then(|playback| playback.item.as_ref());
        let summary = playback_summary(playback);
        let mut main = div()
            .flex_1()
            .h_full()
            .p_10()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(rgb(cx.desktop_theme().bg_root));

        if let Some(item) = item {
            if let Some(image) = self
                .current_artwork_url(true)
                .and_then(|url| self.artwork_cache.get(&url))
            {
                main = main.child(
                    img(image.clone())
                        .w(px(420.))
                        .h(px(420.))
                        .object_fit(gpui::ObjectFit::Cover),
                );
            } else {
                main = main.child(
                    div()
                        .w(px(420.))
                        .h(px(420.))
                        .bg(rgb(cx.desktop_theme().bg_elevated))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(cx.desktop_theme().text_secondary))
                        .child("Artwork loading"),
                );
            }
            main =
                main.child(div().mt_6().text_3xl().child(item.name.clone()))
                    .child(div().mt_2().text_lg().child(media_context_links(
                        "now-playing",
                        item,
                        cx,
                    )))
                    .child(
                        div()
                            .mt_2()
                            .text_sm()
                            .text_color(rgb(cx.desktop_theme().text_muted))
                            .child(summary.progress),
                    );
        } else {
            main = main.child(queue_message(cx, "Nothing is playing"));
        }

        let toggle_label = if self.queue_visible {
            "Hide queue"
        } else {
            "Show queue"
        };
        main = main.child(
            div()
                .id("now-playing-queue-toggle")
                .mt_6()
                .cursor_pointer()
                .rounded_md()
                .bg(rgb(cx.desktop_theme().button_secondary))
                .px_4()
                .py_2()
                .text_sm()
                .flex()
                .items_center()
                .gap_2()
                .child(app_icon(
                    AppIcon::Queue,
                    18.,
                    cx.desktop_theme().text_primary,
                ))
                .child(toggle_label)
                .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
                .on_click(cx.listener(|app, _, _, cx| {
                    app.toggle_queue_rail();
                    cx.notify();
                })),
        );

        let mut pane = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_row()
            .bg(rgb(cx.desktop_theme().bg_root))
            .child(main);
        if self.queue_visible {
            pane = pane.child(self.queue_rail(cx));
        }
        pane
    }

    fn queue_rail(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let mut rows = div().flex().flex_col().gap_2();
        if let Some(queue) = &self.queue {
            if let Some(item) = &queue.currently_playing {
                rows = rows.child(queue_rail_row(
                    cx,
                    "queue-current",
                    "Now playing",
                    item,
                    true,
                    &self.artwork_cache,
                ));
            }
            rows = rows.child(
                div()
                    .mt_4()
                    .text_xs()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("NEXT UP"),
            );
            for (index, item) in queue.items.iter().enumerate() {
                rows = rows.child(queue_rail_row(
                    cx,
                    &format!("queue-item-{index}"),
                    "",
                    item,
                    false,
                    &self.artwork_cache,
                ));
            }
            if queue.items.is_empty() {
                rows = rows.child(queue_message(cx, "No upcoming items"));
            }
        } else {
            rows = rows.child(queue_message(
                cx,
                if self.queue_loading {
                    "Loading queue..."
                } else {
                    "Queue is not loaded yet"
                },
            ));
        }

        div()
            .w(px(310.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(rgb(cx.desktop_theme().border))
            .bg(rgb(cx.desktop_theme().bg_elevated))
            .p_5()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xl().child("Queue"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(cx.desktop_theme().text_secondary))
                            .child("Now Playing"),
                    ),
            )
            .child(
                div()
                    .id("now-playing-queue")
                    .mt_4()
                    .h(px(0.))
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.queue_scroll)
                    .child(rows),
            )
    }

    fn devices_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Devices"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Spotify Connect devices"),
            );

        if self.devices_loading {
            return pane.child(queue_message(cx, "Loading devices..."));
        }
        if self.devices.is_empty() {
            return pane.child(queue_message(cx, "No devices available"));
        }

        let mut rows = div().mt_4().flex().flex_col().gap_2();
        for device in &self.devices {
            let device_id = device.id.clone();
            let status = if device.is_restricted {
                "Restricted"
            } else if device.is_active {
                "Active"
            } else {
                "Available"
            };
            let mut row = div()
                .id(SharedString::from(format!("device-{}", device.name)))
                .rounded_md()
                .border_1()
                .border_color(rgb(cx.desktop_theme().border))
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .child(div().text_sm().child(device.name.clone()))
                        .child(
                            div()
                                .mt_1()
                                .text_xs()
                                .text_color(rgb(cx.desktop_theme().text_muted))
                                .child(device.kind.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(cx.desktop_theme().text_secondary))
                        .child(status),
                );
            if let Some(device_id) = device_id {
                if !device.is_active && !device.is_restricted {
                    row = row.child(search_row_action(
                        cx,
                        "Transfer",
                        cx.listener(move |app, _, _, cx| {
                            app.transfer_to_device(device_id.clone());
                            cx.notify();
                        }),
                    ));
                }
            }
            rows = rows.child(row);
        }
        pane.child(rows)
    }

    fn lyrics_pane(&self, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
        let pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Lyrics"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Lyrics for the current track"),
            );

        if self.lyrics_loading {
            return pane.child(queue_message(cx, "Loading lyrics..."));
        }
        if let Some(error) = &self.lyrics_error {
            return pane.child(queue_message(cx, &format!("Lyrics unavailable: {error}")));
        }
        let Some(lyrics) = &self.lyrics else {
            return pane.child(queue_message(
                cx,
                if self.current_track_uri().is_some() {
                    "Lyrics aren't available for this track"
                } else {
                    "Play a track to see its lyrics"
                },
            ));
        };
        if lyrics.lines.is_empty() {
            return pane.child(queue_message(cx, "Lyrics aren't available for this track"));
        }

        let active = lyrics
            .synced
            .then(|| {
                active_lyric_line_index(
                    &lyrics.lines,
                    self.playback
                        .as_ref()
                        .map(playback_progress_ms)
                        .unwrap_or_default(),
                    self.lyrics_offset_ms,
                )
            })
            .flatten();
        let mut lines = div().mt_6().flex().flex_col().gap_3();
        for (index, line) in lyrics.lines.iter().enumerate() {
            let is_active = active == Some(index);
            lines = lines.child(
                div()
                    .text_color(if is_active {
                        rgb(cx.desktop_theme().text_primary)
                    } else {
                        rgb(cx.desktop_theme().text_muted)
                    })
                    .text_size(px(if is_active { 26. } else { 20. }))
                    .font_weight(if is_active {
                        gpui::FontWeight::BOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(if line.text.is_empty() {
                        "♪".to_string()
                    } else {
                        line.text.clone()
                    }),
            );
        }
        pane.child(if lyrics.synced {
            lines
        } else {
            lines.child(
                div()
                    .mt_4()
                    .text_sm()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("These lyrics are not synchronized."),
            )
        })
    }

    fn search_pane(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let input = self
            .search_input
            .clone()
            .expect("search input should be initialized before rendering");
        let status = if self.search_loading {
            "Searching…".to_string()
        } else if self.search_query.trim().is_empty() {
            "Type a query and press Enter".to_string()
        } else if self.search_results.is_empty() {
            self.search_error
                .clone()
                .unwrap_or_else(|| "No results".to_string())
        } else {
            let result_count = format!(
                "{} result{}",
                self.search_results.len(),
                if self.search_results.len() == 1 {
                    ""
                } else {
                    "s"
                }
            );
            if self.search_error.is_some() {
                format!("{result_count} · some result types unavailable")
            } else {
                result_count
            }
        };

        let mut results = div()
            .id("search-results")
            .mt_5()
            .flex_1()
            // A flex item needs a definite base height here. Without it,
            // GPUI lets the result list grow to its content height, so there
            // is no overflow region for the mouse wheel to scroll.
            .h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&self.search_scroll)
            .flex()
            .flex_col()
            .gap_2();
        for (index, item) in self.search_results.iter().enumerate() {
            results = results.child(search_result_row(index, item, cx));
        }

        let mut pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Search"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child("Find tracks, artists, albums, playlists, and episodes."),
            )
            .child(
                div()
                    .mt_6()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(input))
                    .child(search_button(cx)),
            )
            .child(
                div()
                    .mt_3()
                    .text_sm()
                    .text_color(
                        if self.search_error.is_some() && self.search_results.is_empty() {
                            rgb(cx.desktop_theme().error)
                        } else {
                            rgb(cx.desktop_theme().text_muted)
                        },
                    )
                    .child(status),
            )
            .child(results);

        if self.playlist_picker_uri.is_some() {
            pane = pane.child(playlist_picker_surface(self, cx));
        }

        pane
    }

    fn now_playing_footer(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let summary = playback_summary(self.playback.as_ref());
        let playback = self.playback.as_ref();
        let shuffle_state = playback.is_some_and(|playback| playback.shuffle);
        let repeat_state = playback.map(|playback| playback.repeat).unwrap_or_default();
        let is_playing = playback.is_some_and(|playback| playback.is_playing);
        let current_track_like_status = self.current_track_like_status();
        let footer_context = playback
            .and_then(|playback| playback.item.as_ref())
            .map(|item| media_context_links("footer-current-track", item, cx).into_any_element())
            .unwrap_or_else(|| div().child(summary.subtitle.clone()).into_any_element());

        let footer_artwork = self
            .playback
            .as_ref()
            .and_then(|playback| playback.item.as_ref())
            .and_then(|item| artwork_url(item, true))
            .and_then(|url| self.artwork_cache.get(&url));
        let footer_art = footer_artwork.map(|image| {
            img(image.clone())
                .w_full()
                .h_full()
                .object_fit(gpui::ObjectFit::Cover)
        });

        div()
            .h(px(164.))
            .border_t_1()
            .border_color(rgb(cx.desktop_theme().border))
            .bg(rgb(cx.desktop_theme().bg_surface))
            .px_6()
            .py_4()
            .flex()
            .items_center()
            .gap_6()
            .child(
                div()
                    .w(px(330.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(px(66.))
                            .h(px(66.))
                            .flex_shrink_0()
                            .rounded_md()
                            .overflow_hidden()
                            .when(footer_artwork.is_none(), |element| {
                                element
                                    .bg(rgb(cx.desktop_theme().bg_elevated))
                                    .border_1()
                                    .border_color(rgb(cx.desktop_theme().border_strong))
                            })
                            .when_some(footer_art, |element, art| element.child(art)),
                    )
                    .child(
                        div()
                            .ml_4()
                            .flex_1()
                            .overflow_hidden()
                            .child(div().text_lg().truncate().child(summary.title))
                            .child(
                                div()
                                    .mt_1()
                                    .text_sm()
                                    .text_color(rgb(cx.desktop_theme().text_secondary))
                                    .truncate()
                                    .child(footer_context),
                            ),
                    )
                    .child(footer_like_button(
                        current_track_like_status == Some(true),
                        current_track_like_status.is_some(),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(seek_bar(playback, self.slider_preview, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(transport_button(
                                "previous",
                                "Previous",
                                AppIcon::Previous,
                                PlaybackCommand::Previous,
                                false,
                                false,
                                cx,
                            ))
                            .child(transport_button(
                                "play-pause",
                                if is_playing { "Pause" } else { "Play" },
                                if is_playing {
                                    AppIcon::Pause
                                } else {
                                    AppIcon::Play
                                },
                                if is_playing {
                                    PlaybackCommand::Pause
                                } else {
                                    PlaybackCommand::Resume
                                },
                                false,
                                true,
                                cx,
                            ))
                            .child(transport_button(
                                "next",
                                "Next",
                                AppIcon::Next,
                                PlaybackCommand::Next,
                                false,
                                false,
                                cx,
                            ))
                            .child(transport_button(
                                "shuffle",
                                if shuffle_state {
                                    "Shuffle on"
                                } else {
                                    "Shuffle"
                                },
                                AppIcon::Shuffle,
                                PlaybackCommand::Shuffle {
                                    state: !shuffle_state,
                                },
                                shuffle_state,
                                false,
                                cx,
                            ))
                            .child(transport_button(
                                "repeat",
                                format!("Repeat {}", repeat_state.label()),
                                if repeat_state == RepeatMode::Track {
                                    AppIcon::RepeatOne
                                } else {
                                    AppIcon::Repeat
                                },
                                PlaybackCommand::Repeat {
                                    state: next_repeat_state(repeat_state),
                                },
                                repeat_state != RepeatMode::Off,
                                false,
                                cx,
                            ))
                            .child(footer_queue_button(self.queue_visible, cx)),
                    ),
            )
            .child(
                div()
                    .w(px(210.))
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap_3()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .text_color(rgb(cx.desktop_theme().text_muted))
                            .child(app_icon(
                                AppIcon::Devices,
                                16.,
                                cx.desktop_theme().text_muted,
                            ))
                            .child(
                                div()
                                    .max_w(px(180.))
                                    .text_xs()
                                    .truncate()
                                    .child(summary.device),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_color(rgb(cx.desktop_theme().text_muted))
                            .child(app_icon(
                                AppIcon::Volume,
                                16.,
                                cx.desktop_theme().text_muted,
                            ))
                            .child(volume_bar(playback, self.slider_preview, cx)),
                    ),
            )
    }
}

#[cfg(debug_assertions)]
fn debug_fps_surface(cx: &App, fps: &DebugFps) -> impl IntoElement {
    let color = if fps.frames_per_second >= 55. {
        0x75c991
    } else if fps.frames_per_second >= 30. {
        0xe8b86d
    } else {
        0xe07878
    };
    let label = if fps.frames_per_second > 0. {
        format!(
            "DEBUG  {:.0} FPS  ·  {:.1} ms",
            fps.frames_per_second, fps.frame_time_ms
        )
    } else {
        "DEBUG  measuring FPS…".to_string()
    };

    div()
        .absolute()
        .top_4()
        .right_4()
        .rounded_full()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border_strong))
        .bg(rgb(cx.desktop_theme().bg_elevated))
        .px_3()
        .py_2()
        .text_xs()
        .text_color(rgb(color))
        .child(label)
}

fn queue_message(cx: &App, message: &str) -> impl IntoElement {
    div()
        .mt_6()
        .rounded_lg()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .p_5()
        .text_color(rgb(cx.desktop_theme().text_secondary))
        .child(message.to_string())
}

fn queue_rail_row(
    cx: &mut Context<'_, DesktopApp>,
    id_prefix: &str,
    label: &str,
    item: &MediaItem,
    current: bool,
    artwork_cache: &HashMap<String, Arc<Image>>,
) -> impl IntoElement {
    let image = artwork_url(item, false);
    let mut row = div()
        .rounded_md()
        .bg(rgb(if current {
            cx.desktop_theme().queue_current
        } else {
            cx.desktop_theme().queue_idle
        }))
        .px_3()
        .py_3()
        .flex()
        .items_center()
        .gap_3();
    if let Some(image) = image
        .and_then(|url| artwork_cache.get(&url))
        .or_else(|| artwork_url(item, true).and_then(|url| artwork_cache.get(&url)))
    {
        row = row.child(
            div()
                .w(px(42.))
                .h(px(42.))
                .bg(rgb(cx.desktop_theme().bg_elevated))
                .child(
                    img(image.clone())
                        .w_full()
                        .h_full()
                        .object_fit(gpui::ObjectFit::Cover),
                ),
        );
    } else {
        row = row.child(
            div()
                .w(px(42.))
                .h(px(42.))
                .bg(rgb(cx.desktop_theme().bg_elevated)),
        );
    }
    row.child(
        div()
            .flex_1()
            .overflow_hidden()
            .child(if label.is_empty() {
                div().text_sm().truncate().child(item.name.clone())
            } else {
                div()
                    .text_xs()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .child(label.to_string())
            })
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(rgb(cx.desktop_theme().text_secondary))
                    .truncate()
                    .child(media_context_links(id_prefix, item, cx)),
            ),
    )
}

fn artwork_url(item: &MediaItem, large: bool) -> Option<String> {
    if large {
        item.image_url_large
            .clone()
            .or_else(|| item.image_url.clone())
            .or_else(|| item.image_url_small.clone())
    } else {
        item.image_url_small
            .clone()
            .or_else(|| item.image_url.clone())
            .or_else(|| item.image_url_large.clone())
    }
}

fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(ImageFormat::Webp)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else {
        None
    }
}

fn nav_item(
    destination: Destination,
    selected: bool,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let background = if selected {
        cx.desktop_theme().nav_active
    } else {
        cx.desktop_theme().bg_sidebar
    };
    let foreground = if selected {
        cx.desktop_theme().accent
    } else {
        cx.desktop_theme().text_secondary
    };

    div()
        .id(SharedString::from(format!("nav-{}", destination.id())))
        .mb_1()
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .flex()
        .items_center()
        .gap_3()
        .child(app_icon(destination.icon(), 20., foreground))
        .child(destination.label())
        .hover(|style| style.bg(rgb(cx.desktop_theme().nav_hover)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.select_destination(destination);
            cx.notify();
        }))
}

fn search_button(cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    div()
        .id("search-submit")
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(cx.desktop_theme().accent))
        .px_4()
        .py_3()
        .text_sm()
        .text_color(rgb(cx.desktop_theme().text_primary))
        .hover(|style| style.bg(rgb(cx.desktop_theme().accent_hover)))
        .on_click(cx.listener(|app, _, _, cx| {
            app.start_search();
            cx.notify();
        }))
        .child("Search")
}

fn catalog_link(
    id: String,
    label: String,
    target: MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .cursor_pointer()
        .text_color(rgb(cx.desktop_theme().accent))
        .hover(|style| style.text_color(rgb(cx.desktop_theme().accent_hover)))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _, _, cx| {
                match target.kind {
                    MediaKind::Album => app.navigate_to_album(target.clone()),
                    MediaKind::Artist => app.navigate_to_artist(target.clone()),
                    _ => {}
                }
                cx.notify();
            }),
        )
        .child(label)
}

fn media_context_links(
    id_prefix: &str,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let mut links = div()
        .flex()
        .items_center()
        .gap_1()
        .overflow_hidden()
        .text_color(rgb(cx.desktop_theme().text_muted));
    let mut has_content = false;

    if item.artists.is_empty() {
        if !item.subtitle.is_empty() {
            links = links.child(div().truncate().child(item.subtitle.clone()));
            has_content = true;
        }
    } else {
        for (index, artist) in item.artists.iter().enumerate() {
            if index > 0 {
                links = links.child("·");
            }
            links = links.child(catalog_link(
                format!("{id_prefix}-artist-{index}"),
                artist.name.clone(),
                MediaItem {
                    name: artist.name.clone(),
                    uri: artist.uri.clone(),
                    kind: MediaKind::Artist,
                    ..MediaItem::default()
                },
                cx,
            ));
            has_content = true;
        }
    }

    if item.kind != MediaKind::Album {
        if let (Some(album_uri), Some(album_name)) = (&item.album_uri, &item.album) {
            if has_content {
                links = links.child("·");
            }
            links = links.child(catalog_link(
                format!("{id_prefix}-album"),
                album_name.clone(),
                MediaItem {
                    name: album_name.clone(),
                    uri: album_uri.clone(),
                    kind: MediaKind::Album,
                    artists: item.artists.clone(),
                    ..MediaItem::default()
                },
                cx,
            ));
            has_content = true;
        }
    }

    if !has_content {
        links = links.child(div().truncate().child(media_subtitle(item)));
    }
    links
}

fn search_result_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled result".to_string()
    } else {
        item.name.clone()
    };
    let play_uri = uri.clone();
    let play_title = title.clone();
    let queue_uri = uri.clone();
    let queue_title = title.clone();
    let add_uri = uri.clone();
    let can_queue = matches!(
        item.kind,
        MediaKind::Track | MediaKind::Episode | MediaKind::Album | MediaKind::Playlist
    );
    let can_add_to_playlist = matches!(item.kind, MediaKind::Track | MediaKind::Episode);
    let catalog_target =
        matches!(item.kind, MediaKind::Album | MediaKind::Artist).then(|| item.clone());

    let mut row = div()
        .id(SharedString::from(format!("search-result-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .child(if let Some(target) = catalog_target {
                    catalog_link(
                        format!("search-result-{index}-catalog-link"),
                        title,
                        target,
                        cx,
                    )
                    .into_any_element()
                } else {
                    div()
                        .text_sm()
                        .text_color(rgb(cx.desktop_theme().text_primary))
                        .truncate()
                        .child(title)
                        .into_any_element()
                })
                .child(div().mt_1().text_xs().child(media_context_links(
                    &format!("search-result-{index}"),
                    item,
                    cx,
                ))),
        )
        .child(search_row_action(
            cx,
            "Play",
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ));

    if can_queue {
        row = row.child(search_row_action(
            cx,
            "Queue",
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd {
                    uri: queue_uri.clone(),
                });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ));
    }

    if can_add_to_playlist {
        row = row.child(search_row_action(
            cx,
            "Add to playlist",
            cx.listener(move |app, _, _, cx| {
                app.open_playlist_picker(add_uri.clone());
                cx.notify();
            }),
        ));
    }

    row
}

fn album_row(index: usize, item: &MediaItem, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    let title = if item.name.is_empty() {
        "Untitled album".to_string()
    } else {
        item.name.clone()
    };
    let uri = item.uri.clone();
    let can_act = !uri.is_empty();
    let mut row = div()
        .id(SharedString::from(format!("album-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .child(catalog_link(
                    format!("album-{index}-link"),
                    title.clone(),
                    item.clone(),
                    cx,
                ))
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(rgb(cx.desktop_theme().text_muted))
                        .truncate()
                        .child(media_context_links(&format!("album-{index}"), item, cx)),
                ),
        );

    if can_act {
        let open_album = item.clone();
        let play_uri = uri.clone();
        let queue_uri = uri;
        let play_title = title.clone();
        let queue_title = title;
        row = row
            .child(search_row_action(
                cx,
                "Open",
                cx.listener(move |app, _, _, cx| {
                    app.navigate_to_album(open_album.clone());
                    cx.notify();
                }),
            ))
            .child(search_row_action(
                cx,
                "Play",
                cx.listener(move |app, _, _, cx| {
                    app.send_playback_command(PlaybackCommand::PlayUri {
                        uri: play_uri.clone(),
                        context_uri: None,
                    });
                    app.toast = Some(format!("Playing {play_title}"));
                    cx.notify();
                }),
            ))
            .child(search_row_action(
                cx,
                "Queue",
                cx.listener(move |app, _, _, cx| {
                    app.send_request(Request::QueueAdd {
                        uri: queue_uri.clone(),
                    });
                    app.toast = Some(format!("Queued {queue_title}"));
                    cx.notify();
                }),
            ));
    }
    row
}

fn artist_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let title = if item.name.is_empty() {
        "Unnamed artist"
    } else {
        item.name.as_str()
    };
    let artist = item.clone();
    div()
        .id(SharedString::from(format!("artist-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(div().flex_1().child(catalog_link(
            format!("artist-{index}-link"),
            title.to_string(),
            item.clone(),
            cx,
        )))
        .child(search_row_action(
            cx,
            "Open",
            cx.listener(move |app, _, _, cx| {
                app.navigate_to_artist(artist.clone());
                cx.notify();
            }),
        ))
}

fn detail_album_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled album".to_string()
    } else {
        item.name.clone()
    };
    let play_uri = uri.clone();
    let queue_uri = uri;
    let play_title = title.clone();
    let queue_title = title.clone();

    div()
        .id(SharedString::from(format!("artist-album-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .child(catalog_link(
                    format!("artist-album-{index}-link"),
                    title,
                    item.clone(),
                    cx,
                ))
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(rgb(cx.desktop_theme().text_muted))
                        .child(media_context_links(
                            &format!("artist-album-{index}"),
                            item,
                            cx,
                        )),
                ),
        )
        .child(search_row_action(
            cx,
            "Play",
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ))
        .child(search_row_action(
            cx,
            "Queue",
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd {
                    uri: queue_uri.clone(),
                });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ))
}

fn liked_song_row(
    index: usize,
    item: &MediaItem,
    artwork_cache: &HashMap<String, Arc<Image>>,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled song".to_string()
    } else {
        item.name.clone()
    };
    let artwork = artwork_url(item, false)
        .and_then(|url| artwork_cache.get(&url))
        .or_else(|| artwork_url(item, true).and_then(|url| artwork_cache.get(&url)));
    let play_uri = uri.clone();
    let play_title = title.clone();
    let queue_uri = uri;
    let queue_title = title.clone();

    div()
        .id(SharedString::from(format!("liked-song-{index}")))
        .w_full()
        .h(px(56.))
        .rounded_md()
        .px_2()
        .flex()
        .items_center()
        .gap_3()
        .hover(|style| style.bg(rgb(cx.desktop_theme().bg_elevated)))
        .child(
            div()
                .size(px(42.))
                .flex_shrink_0()
                .rounded_sm()
                .overflow_hidden()
                .bg(rgb(cx.desktop_theme().bg_elevated))
                .flex()
                .items_center()
                .justify_center()
                .when_some(artwork.cloned(), |element, artwork| {
                    element.child(
                        img(artwork)
                            .w_full()
                            .h_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                })
                .when(artwork.is_none(), |element| {
                    element.child(app_icon(
                        AppIcon::NowPlaying,
                        17.,
                        cx.desktop_theme().text_muted,
                    ))
                }),
        )
        .child(liked_song_action(
            cx,
            format!("liked-song-{index}-play"),
            "Play",
            AppIcon::Play,
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ))
        .child(liked_song_action(
            cx,
            format!("liked-song-{index}-queue"),
            "Add to queue",
            AppIcon::Queue,
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd {
                    uri: queue_uri.clone(),
                });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ))
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(cx.desktop_theme().text_primary))
                        .truncate()
                        .child(title),
                )
                .child(div().mt_1().text_xs().child(media_context_links(
                    &format!("liked-song-{index}"),
                    item,
                    cx,
                ))),
        )
}

fn liked_song_action(
    cx: &App,
    id: String,
    label: &'static str,
    icon: AppIcon,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .size(px(34.))
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(rgb(cx.desktop_theme().text_secondary))
        .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
        .tooltip(icon_tooltip(label))
        .on_mouse_up(MouseButton::Left, listener)
        .child(app_icon(icon, 16., cx.desktop_theme().text_secondary))
}

fn history_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled track".to_string()
    } else {
        item.name.clone()
    };
    let play_uri = uri.clone();
    let play_title = title.clone();
    let queue_title = title.clone();

    div()
        .id(SharedString::from(format!("history-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(div().flex_1().child(div().text_sm().child(title)).child(
            div().mt_1().text_xs().child(media_context_links(
                &format!("history-{index}"),
                item,
                cx,
            )),
        ))
        .child(search_row_action(
            cx,
            "Play",
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ))
        .child(search_row_action(
            cx,
            "Queue",
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd { uri: uri.clone() });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ))
}

fn playlist_row(
    index: usize,
    playlist: &Playlist,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let playlist = playlist.clone();
    div()
        .id(SharedString::from(format!("playlist-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .child(div().text_sm().child(playlist.name.clone()))
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(rgb(cx.desktop_theme().text_muted))
                        .child(format!(
                            "{} tracks · {}",
                            playlist.tracks_total, playlist.owner
                        )),
                ),
        )
        .child(search_row_action(
            cx,
            "Open",
            cx.listener(move |app, _, _, cx| {
                app.open_playlist(playlist.clone());
                cx.notify();
            }),
        ))
}

fn playlist_track_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled track".to_string()
    } else {
        item.name.clone()
    };
    let play_uri = uri.clone();
    let play_title = title.clone();
    let queue_title = title.clone();

    div()
        .id(SharedString::from(format!("playlist-track-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border))
        .bg(rgb(cx.desktop_theme().bg_surface))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(div().flex_1().child(div().text_sm().child(title)).child(
            div().mt_1().text_xs().child(media_context_links(
                &format!("playlist-track-{index}"),
                item,
                cx,
            )),
        ))
        .child(search_row_action(
            cx,
            "Play",
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ))
        .child(search_row_action(
            cx,
            "Queue",
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd { uri: uri.clone() });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ))
}

fn search_row_action(
    cx: &App,
    label: &'static str,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border_strong))
        .px_3()
        .py_2()
        .text_xs()
        .text_color(rgb(cx.desktop_theme().text_primary))
        .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
        .on_mouse_up(MouseButton::Left, listener)
        .child(label)
}

fn playlist_picker_surface(app: &DesktopApp, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    let mut picker = div()
        .mt_5()
        .rounded_lg()
        .border_1()
        .border_color(rgb(cx.desktop_theme().error_border))
        .bg(rgb(cx.desktop_theme().error_surface))
        .p_4()
        .child(div().text_sm().child("Choose a playlist"));

    if app.playlist_loading {
        picker = picker.child(
            div()
                .mt_2()
                .text_xs()
                .text_color(rgb(cx.desktop_theme().text_muted))
                .child("Loading playlists…"),
        );
    } else if app.search_playlists.is_empty() {
        picker = picker.child(
            div()
                .mt_2()
                .text_xs()
                .text_color(rgb(cx.desktop_theme().text_muted))
                .child("No playlists available"),
        );
    } else {
        let mut playlists = div().mt_3().flex().flex_wrap().gap_2();
        for playlist in &app.search_playlists {
            let playlist_id = playlist.id.clone();
            let playlist_name = playlist.name.clone();
            playlists = playlists.child(
                div()
                    .id(SharedString::from(format!(
                        "playlist-picker-{}",
                        playlist.id
                    )))
                    .cursor_pointer()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(cx.desktop_theme().border_strong))
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(cx.desktop_theme().text_primary))
                    .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |app, _, _, cx| {
                            app.add_search_result_to_playlist(playlist_id.clone());
                            cx.notify();
                        }),
                    )
                    .child(playlist_name),
            );
        }
        picker = picker.child(playlists);
    }

    picker.child(
        div()
            .mt_3()
            .cursor_pointer()
            .text_xs()
            .text_color(rgb(cx.desktop_theme().accent_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.playlist_picker_uri = None;
                    app.playlist_loading = false;
                    cx.notify();
                }),
            )
            .child("Cancel"),
    )
}

struct SearchInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    desktop_app: WeakEntity<DesktopApp>,
    cursor: usize,
    selected_range: Range<usize>,
    selection_anchor: Option<usize>,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl SearchInput {
    fn new(cx: &mut Context<Self>, desktop_app: WeakEntity<DesktopApp>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: "Search Spotify".into(),
            desktop_app,
            cursor: 0,
            selected_range: 0..0,
            selection_anchor: None,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
        }
    }

    fn notify_owner(&self, cx: &mut Context<Self>) {
        let query = self.content.to_string();
        let _ = self.desktop_app.update(cx, |app, cx| {
            app.update_search_query(query);
            cx.notify();
        });
    }

    fn submit(&self, cx: &mut Context<Self>) {
        let _ = self.desktop_app.update(cx, |app, cx| {
            app.start_search();
            cx.notify();
        });
    }

    fn cursor_offset(&self) -> usize {
        self.cursor
    }

    fn set_cursor(&mut self, offset: usize, extend_selection: bool) {
        (self.cursor, self.selected_range, self.selection_anchor) = selection_after_cursor_move(
            self.cursor,
            self.selection_anchor,
            offset,
            extend_selection,
            self.content.len(),
        );
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection_anchor = Some(0);
        self.selected_range = 0..self.content.len();
        self.cursor = self.content.len();
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| offset + index)
            .unwrap_or(self.content.len())
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        let mut boundary = offset;
        let mut saw_word = false;
        for (index, ch) in self.content[..offset].char_indices().rev() {
            if ch.is_whitespace() {
                if saw_word {
                    return boundary;
                }
            } else {
                saw_word = true;
            }
            boundary = index;
        }
        0
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        let mut boundary = offset;
        let mut saw_word = false;
        for (index, ch) in self.content[offset..].char_indices() {
            if ch.is_whitespace() {
                if saw_word {
                    return offset + index;
                }
            } else {
                saw_word = true;
            }
            boundary = offset + index + ch.len_utf8();
        }
        boundary
    }

    fn replace_text(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            text,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + text.len();
        self.cursor = cursor;
        self.selected_range = cursor..cursor;
        self.selection_anchor = None;
        self.marked_range = None;
        self.notify_owner(cx);
        cx.notify();
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            self.previous_boundary(cursor)..cursor
        } else {
            self.selected_range.clone()
        };
        if !range.is_empty() {
            self.replace_text(range, "", cx);
        }
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            cursor..self.next_boundary(cursor)
        } else {
            self.selected_range.clone()
        };
        if !range.is_empty() {
            self.replace_text(range, "", cx);
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (&self.last_bounds, &self.last_layout) else {
            return self.content.len();
        };
        line.index_for_x(position.x - bounds.left())
            .unwrap_or(self.content.len())
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        let extend_selection = modifiers.shift;
        let secondary = modifiers.secondary();
        let word_navigation = modifiers.alt || (modifiers.control && !cfg!(target_os = "macos"));

        if secondary && event.keystroke.key.eq_ignore_ascii_case("a") {
            self.select_all(cx);
            return;
        }

        match event.keystroke.key.as_str() {
            "enter" => self.submit(cx),
            "backspace" => self.backspace(cx),
            "delete" => self.delete(cx),
            "left" => {
                let offset = if !extend_selection && !self.selected_range.is_empty() {
                    self.selected_range.start
                } else if secondary {
                    0
                } else if word_navigation {
                    self.previous_word_boundary(self.cursor_offset())
                } else {
                    self.previous_boundary(self.cursor_offset())
                };
                self.set_cursor(offset, extend_selection);
                cx.notify();
            }
            "right" => {
                let offset = if !extend_selection && !self.selected_range.is_empty() {
                    self.selected_range.end
                } else if secondary {
                    self.content.len()
                } else if word_navigation {
                    self.next_word_boundary(self.cursor_offset())
                } else {
                    self.next_boundary(self.cursor_offset())
                };
                self.set_cursor(offset, extend_selection);
                cx.notify();
            }
            "home" => {
                self.set_cursor(0, extend_selection);
                cx.notify();
            }
            "end" => {
                self.set_cursor(self.content.len(), extend_selection);
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window);
        self.set_cursor(
            self.index_for_mouse_position(event.position),
            event.modifiers.shift,
        );
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {}

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        for (index, ch) in self.content.char_indices() {
            if utf16_offset >= offset {
                return index;
            }
            utf16_offset += ch.len_utf16();
        }
        self.content.len()
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

fn selection_after_cursor_move(
    cursor: usize,
    selection_anchor: Option<usize>,
    offset: usize,
    extend_selection: bool,
    content_len: usize,
) -> (usize, Range<usize>, Option<usize>) {
    let offset = offset.min(content_len);
    if extend_selection {
        let anchor = selection_anchor.unwrap_or(cursor);
        (offset, anchor.min(offset)..anchor.max(offset), Some(anchor))
    } else {
        (offset, offset..offset, None)
    }
}

impl EntityInputHandler for SearchInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: !self.selected_range.is_empty() && self.cursor == self.selected_range.start,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.replace_text(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selected| self.range_from_utf16(selected))
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.cursor = self.selected_range.end;
        self.selection_anchor = None;
        self.notify_owner(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.index_for_mouse_position(point))
    }
}

impl Focusable for SearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

struct SearchTextElement {
    input: Entity<SearchInput>,
}

struct SearchTextPrepaint {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for SearchTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SearchTextElement {
    type RequestLayoutState = ();
    type PrepaintState = SearchTextPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let display_text = if input.content.is_empty() {
            input.placeholder.clone()
        } else {
            input.content.clone()
        };
        let text_color = if input.content.is_empty() {
            window.text_style().color.opacity(0.55)
        } else {
            window.text_style().color
        };
        let run = TextRun {
            len: display_text.len(),
            font: window.text_style().font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);
        let cursor_pos = line.x_for_index(input.cursor_offset());
        let cursor = if input.focus_handle.is_focused(window) {
            Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_pos, bounds.top()),
                    gpui::size(px(2.), bounds.bottom() - bounds.top()),
                ),
                rgb(cx.desktop_theme().accent),
            ))
        } else {
            None
        };
        let selection = (!input.selected_range.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(input.selected_range.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(input.selected_range.end),
                        bounds.bottom(),
                    ),
                ),
                rgb(cx.desktop_theme().button_secondary_hover),
            )
        });
        SearchTextPrepaint {
            line,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        prepaint
            .line
            .paint(bounds.origin, window.line_height(), window, cx)
            .expect("search input text should paint");
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(prepaint.line.clone());
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for SearchInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("search-input")
            .debug_selector(|| "search-input".to_string())
            .h(px(42.))
            .flex()
            .key_context("SearchInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .rounded_md()
            .border_1()
            .border_color(rgb(cx.desktop_theme().border_strong))
            .bg(rgb(cx.desktop_theme().bg_sidebar))
            .px_3()
            .items_center()
            .text_sm()
            .child(SearchTextElement { input: cx.entity() })
    }
}

fn update_banner_surface(cx: &App, banner: &UpdateBanner) -> impl IntoElement {
    let detail = banner
        .command
        .as_deref()
        .or(banner.release_url.as_deref())
        .unwrap_or("Open the latest release to upgrade.");

    div()
        .mx_6()
        .mt_5()
        .rounded_lg()
        .border_1()
        .border_color(rgb(cx.desktop_theme().error_border))
        .bg(rgb(cx.desktop_theme().accent_subtle))
        .px_4()
        .py_3()
        .child(format!("Update available: {}", banner.latest_version))
        .child(
            div()
                .mt_1()
                .text_sm()
                .text_color(rgb(cx.desktop_theme().accent_hover))
                .child(detail.to_string()),
        )
}

fn toast_surface(cx: &App, message: &str) -> impl IntoElement {
    div()
        .absolute()
        .right_6()
        .bottom(px(180.))
        .max_w(px(420.))
        .rounded_lg()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border_strong))
        .bg(rgb(cx.desktop_theme().bg_elevated))
        .px_4()
        .py_3()
        .text_sm()
        .text_color(rgb(cx.desktop_theme().text_primary))
        .child(message.to_string())
}

const SLIDER_WIDTH: f32 = 420.0;
const LAST_EVENT_WIDTH: f32 = 360.0;
const MAX_EVENT_LABEL_CHARS: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SliderKind {
    Seek,
    Volume,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SliderPreview {
    Seek(u64),
    Volume(u8),
}

impl SliderPreview {
    fn action(self) -> &'static str {
        match self {
            Self::Seek(_) => "seek",
            Self::Volume(_) => "volume",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SeekDrag;

#[derive(Clone, Copy, Debug)]
struct VolumeDrag;

struct SliderGhost;

impl Render for SliderGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_0()
    }
}

fn footer_like_button(
    liked: bool,
    enabled: bool,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let label = if liked { "Unlike" } else { "Like" };
    let foreground = if liked {
        cx.desktop_theme().accent
    } else {
        cx.desktop_theme().text_secondary
    };

    div()
        .id("footer-like")
        .ml_3()
        .size(px(38.))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border_strong))
        .bg(rgb(if liked {
            cx.desktop_theme().nav_active
        } else {
            cx.desktop_theme().bg_elevated
        }))
        .text_color(rgb(foreground))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(!enabled, |element| element.opacity(0.45))
        .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
        .tooltip(icon_tooltip(label))
        .on_click(cx.listener(|app, _, _, cx| {
            app.toggle_current_track_like();
            cx.notify();
        }))
        .child(app_icon(
            if liked {
                AppIcon::LikedSongsFilled
            } else {
                AppIcon::LikedSongs
            },
            19.,
            foreground,
        ))
}

fn footer_queue_button(queue_visible: bool, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    let foreground = if queue_visible {
        cx.desktop_theme().accent
    } else {
        cx.desktop_theme().text_primary
    };
    div()
        .id("transport-queue")
        .cursor_pointer()
        .size(px(38.))
        .rounded_full()
        .border_1()
        .border_color(rgb(cx.desktop_theme().border_strong))
        .bg(rgb(if queue_visible {
            cx.desktop_theme().nav_active
        } else {
            cx.desktop_theme().bg_elevated
        }))
        .flex()
        .items_center()
        .justify_center()
        .hover(|style| style.bg(rgb(cx.desktop_theme().button_secondary_hover)))
        .tooltip(icon_tooltip(if queue_visible {
            "Hide queue"
        } else {
            "Show queue"
        }))
        .on_click(cx.listener(|app, _, _, cx| {
            app.toggle_queue_rail();
            cx.notify();
        }))
        .child(app_icon(AppIcon::Queue, 18., foreground))
}

fn transport_button(
    id: &'static str,
    label: impl Into<SharedString>,
    icon: AppIcon,
    command: PlaybackCommand,
    active: bool,
    primary: bool,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let label = label.into();
    let size = if primary { 46. } else { 38. };
    let background = if primary {
        cx.desktop_theme().accent
    } else if active {
        cx.desktop_theme().nav_active
    } else {
        cx.desktop_theme().bg_elevated
    };
    let foreground = if primary {
        cx.desktop_theme().bg_root
    } else if active {
        cx.desktop_theme().accent
    } else {
        cx.desktop_theme().text_primary
    };
    let hover_background = if primary {
        cx.desktop_theme().accent_hover
    } else {
        cx.desktop_theme().button_secondary_hover
    };

    div()
        .id(SharedString::from(format!("transport-{id}")))
        .cursor_pointer()
        .size(px(size))
        .rounded_full()
        .border_1()
        .border_color(rgb(if primary {
            cx.desktop_theme().accent
        } else {
            cx.desktop_theme().border_strong
        }))
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(rgb(hover_background)))
        .tooltip(icon_tooltip(label))
        .on_click(cx.listener(move |app, _, _, _| {
            app.send_playback_command(command.clone());
        }))
        .child(app_icon(icon, if primary { 22. } else { 18. }, foreground))
}

fn seek_bar(
    playback: Option<&Playback>,
    preview: Option<SliderPreview>,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let fraction = playback
        .and_then(|playback| playback.item.as_ref().map(|item| (playback, item)))
        .filter(|(_, item)| item.duration_ms > 0)
        .map_or(0.0, |(playback, item)| {
            let position_ms = match preview {
                Some(SliderPreview::Seek(position_ms)) => position_ms,
                _ => playback_progress_ms(playback),
            };
            (position_ms as f32 / item.duration_ms as f32).clamp(0.0, 1.0)
        });

    let bounds_owner = cx.entity().downgrade();
    let bar = div()
        .id("seek-bar")
        .debug_selector(|| "seek-bar".to_string())
        .w(px(SLIDER_WIDTH))
        .h(px(14.))
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(cx.desktop_theme().slider_track))
        .child(
            div()
                .h_full()
                .w(px(SLIDER_WIDTH * fraction))
                .rounded_md()
                .bg(rgb(cx.desktop_theme().accent)),
        )
        .on_drag(SeekDrag, |_, _, _, cx| cx.new(|_| SliderGhost))
        .on_drag_move(cx.listener(|app, event: &DragMoveEvent<SeekDrag>, _, cx| {
            if let Some(item) = app
                .playback
                .as_ref()
                .and_then(|playback| playback.item.as_ref())
            {
                let position_ms = seek_position_ms(item.duration_ms, slider_fraction(event));
                app.preview_slider(SliderKind::Seek, SliderPreview::Seek(position_ms));
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _, _, cx| {
                app.finish_slider_drag(SliderKind::Seek);
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|app, _, _, cx| {
                app.finish_slider_drag(SliderKind::Seek);
                cx.notify();
            }),
        );

    div()
        .on_children_prepainted(move |bounds, _, app_cx| {
            let bounds = bounds.first().copied();
            let _ = bounds_owner.update(app_cx, |app, _| {
                app.seek_bar_bounds = bounds;
            });
        })
        .id("seek-hit-area")
        .on_click(cx.listener(|app, event: &ClickEvent, _, cx| {
            let Some(bounds) = app.seek_bar_bounds else {
                return;
            };
            let Some(item) = app
                .playback
                .as_ref()
                .and_then(|playback| playback.item.as_ref())
            else {
                return;
            };
            let position_ms = seek_position_ms(
                item.duration_ms,
                slider_fraction_at(event.position(), bounds),
            );
            app.preview_slider(SliderKind::Seek, SliderPreview::Seek(position_ms));
            app.finish_slider_drag(SliderKind::Seek);
            cx.notify();
        }))
        .child(bar)
}

fn volume_bar(
    playback: Option<&Playback>,
    preview: Option<SliderPreview>,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let device = playback.and_then(|playback| playback.device.as_ref());
    let fraction = device
        .and_then(|device| device.volume_percent)
        .map_or(0.0, |volume| f32::from(volume) / 100.0);
    let fraction = match preview {
        Some(SliderPreview::Volume(volume_percent)) => f32::from(volume_percent) / 100.0,
        _ => fraction,
    };
    let supports_volume = device.is_some_and(|device| device.supports_volume);

    let bounds_owner = cx.entity().downgrade();
    let bar = div()
        .id("volume-bar")
        .debug_selector(|| "volume-bar".to_string())
        .w(px(140.))
        .h(px(10.))
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(cx.desktop_theme().slider_track))
        .child(
            div()
                .h_full()
                .w(px(140. * fraction))
                .rounded_md()
                .bg(rgb(cx.desktop_theme().accent)),
        );

    if supports_volume {
        let bar = bar
            .on_drag(VolumeDrag, |_, _, _, cx| cx.new(|_| SliderGhost))
            .on_drag_move(
                cx.listener(|app, event: &DragMoveEvent<VolumeDrag>, _, cx| {
                    let volume_percent = volume_percent(slider_fraction(event));
                    app.preview_slider(SliderKind::Volume, SliderPreview::Volume(volume_percent));
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.finish_slider_drag(SliderKind::Volume);
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.finish_slider_drag(SliderKind::Volume);
                    cx.notify();
                }),
            );
        div()
            .on_children_prepainted(move |bounds, _, app_cx| {
                let bounds = bounds.first().copied();
                let _ = bounds_owner.update(app_cx, |app, _| {
                    app.volume_bar_bounds = bounds;
                });
            })
            .id("volume-hit-area")
            .on_click(cx.listener(|app, event: &ClickEvent, _, cx| {
                let Some(bounds) = app.volume_bar_bounds else {
                    return;
                };
                let volume_percent = volume_percent(slider_fraction_at(event.position(), bounds));
                app.preview_slider(SliderKind::Volume, SliderPreview::Volume(volume_percent));
                app.finish_slider_drag(SliderKind::Volume);
                cx.notify();
            }))
            .child(bar)
    } else {
        bar.opacity(0.45)
    }
}

fn slider_fraction<T>(event: &DragMoveEvent<T>) -> f32 {
    slider_fraction_at(event.event.position, event.bounds)
}

fn slider_fraction_at(position: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
    ((position.x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0)
}

fn slider_preview_matches_playback(preview: SliderPreview, playback: &Playback) -> bool {
    match preview {
        SliderPreview::Seek(position_ms) => playback.progress_ms.abs_diff(position_ms) <= 1_500,
        SliderPreview::Volume(volume_percent) => {
            playback
                .device
                .as_ref()
                .and_then(|device| device.volume_percent)
                == Some(volume_percent)
        }
    }
}

fn next_repeat_state(repeat: RepeatMode) -> RepeatMode {
    match repeat {
        RepeatMode::Off => RepeatMode::Context,
        RepeatMode::Context => RepeatMode::Track,
        RepeatMode::Track => RepeatMode::Off,
    }
}

fn seek_position_ms(duration_ms: u64, fraction: f32) -> u64 {
    (duration_ms as f32 * fraction.clamp(0.0, 1.0)).round() as u64
}

fn volume_percent(fraction: f32) -> u8 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u8
}

struct PlaybackSummary {
    title: String,
    subtitle: String,
    progress: String,
    device: String,
}

fn playback_summary(playback: Option<&Playback>) -> PlaybackSummary {
    let Some(playback) = playback else {
        return PlaybackSummary {
            title: "Nothing playing".to_string(),
            subtitle: "Waiting for daemon playback state".to_string(),
            progress: "0:00".to_string(),
            device: "No active device".to_string(),
        };
    };

    let item = playback.item.as_ref();
    let title = item
        .map(|item| item.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown track".to_string());
    let subtitle = item
        .map(media_subtitle)
        .unwrap_or_else(|| "No media item".to_string());
    let duration_ms = item.map_or(0, |item| item.duration_ms);
    let progress_ms = playback_progress_ms(playback);
    let progress = if duration_ms == 0 {
        format_duration(progress_ms)
    } else {
        format!(
            "{} / {}",
            format_duration(progress_ms),
            format_duration(duration_ms)
        )
    };
    let device = playback
        .device
        .as_ref()
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "No active device".to_string());

    PlaybackSummary {
        title,
        subtitle,
        progress,
        device,
    }
}

fn playback_progress_ms(playback: &Playback) -> u64 {
    if !playback.is_playing {
        return playback.progress_ms;
    }

    let Some(sampled_at_ms) = playback.sampled_at_ms else {
        return playback.progress_ms;
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(sampled_at_ms, |duration| duration.as_millis() as i64);
    let elapsed_ms = now_ms.saturating_sub(sampled_at_ms) as u64;
    let projected = playback.progress_ms.saturating_add(elapsed_ms);
    playback
        .item
        .as_ref()
        .map_or(projected, |item| projected.min(item.duration_ms))
}

fn media_subtitle(item: &MediaItem) -> String {
    if !item.subtitle.is_empty() {
        item.subtitle.clone()
    } else if !item.context.is_empty() {
        item.context.clone()
    } else {
        item.kind.to_string()
    }
}

fn search_kind_order(kind: &MediaKind) -> u8 {
    match kind {
        MediaKind::Track => 0,
        MediaKind::Episode => 1,
        MediaKind::Show => 2,
        MediaKind::Album => 3,
        MediaKind::Artist => 4,
        MediaKind::Playlist => 5,
    }
}

fn sort_search_results(items: &mut [MediaItem]) {
    // SearchStream fans out one request per media kind. Completion order is
    // intentionally concurrent, so restore a useful, deterministic order at
    // the client boundary and keep tracks in the first visible group.
    items.sort_by_key(|item| search_kind_order(&item.kind));
}

fn format_duration(ms: u64) -> String {
    let total_seconds = ms / 1_000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn event_label(event: &DaemonEvent) -> String {
    let label = match event {
        DaemonEvent::ShutdownRequested => "shutdown-requested".to_string(),
        DaemonEvent::PlaybackChanged { action, .. } => format!("playback:{action}"),
        DaemonEvent::QueueChanged { action, .. } => format!("queue:{action}"),
        DaemonEvent::DevicesChanged { action, .. } => format!("devices:{action}"),
        DaemonEvent::PlaylistsChanged { action, .. } => format!("playlists:{action}"),
        DaemonEvent::LibraryChanged { action, .. } => format!("library:{action}"),
        DaemonEvent::SearchUpdated { count, .. } => format!("search-updated:{count}"),
        DaemonEvent::SearchPage { kind, offset, .. } => {
            format!("search-page:{kind:?}:{offset}")
        }
        DaemonEvent::SearchComplete { .. } => "search-complete".to_string(),
        DaemonEvent::SearchFailed { .. } => "search-failed".to_string(),
        DaemonEvent::EventStreamLagged { skipped } => format!("event-stream-lagged:{skipped}"),
        DaemonEvent::SyncStarted { target, .. } => format!("sync-started:{}", target.label()),
        DaemonEvent::SyncFinished { summary } => {
            format!("sync-finished:{}", summary.target.label())
        }
        DaemonEvent::MutationFinished { action, .. } => format!("mutation:{action}"),
        DaemonEvent::RateLimited { scope, .. } => format!("rate-limited:{scope}"),
        DaemonEvent::AuthError { kind, .. } => format!("auth-error:{kind:?}"),
        DaemonEvent::MutationAccepted { action, .. } => format!("mutation-accepted:{action}"),
        DaemonEvent::MutationFinalized { .. } => "mutation-finalized".to_string(),
        DaemonEvent::SchemaCompat { endpoint, .. } => format!("schema-compat:{endpoint}"),
        DaemonEvent::PlayerReady { name, .. } => format!("player-ready:{name}"),
        DaemonEvent::PlayerDegraded { .. } => "player-degraded".to_string(),
        DaemonEvent::PremiumRequired => "premium-required".to_string(),
        DaemonEvent::SessionDisconnected { .. } => "session-disconnected".to_string(),
        DaemonEvent::PlayerFailed { .. } => "player-failed".to_string(),
        DaemonEvent::ListenQualified { .. } => "listen-qualified".to_string(),
        DaemonEvent::AnalyticsImportProgress { phase, .. } => {
            format!("analytics-import:{phase}")
        }
        DaemonEvent::OperationRecorded { .. } => "operation-recorded".to_string(),
        DaemonEvent::OperationUndone { success, .. } => {
            format!(
                "operation-undone:{}",
                if *success { "ok" } else { "failed" }
            )
        }
        DaemonEvent::ConfigReloaded => "config-reloaded".to_string(),
        DaemonEvent::SpectrumFrame { .. } => "spectrum-frame".to_string(),
        DaemonEvent::VizSourceChanged { .. } => "viz-source-changed".to_string(),
        DaemonEvent::ReminderDue { .. } => "reminder-due".to_string(),
        DaemonEvent::RemindersChanged { action } => format!("reminders:{action}"),
        DaemonEvent::UpdateAvailable { latest_version, .. } => {
            format!("update-available:{latest_version}")
        }
        DaemonEvent::ProviderPolicy { provider, .. } => {
            format!("provider-policy:{}", provider.as_str())
        }
        DaemonEvent::ProviderPolicyCleared { provider, .. } => {
            format!("provider-policy-cleared:{}", provider.as_str())
        }
        DaemonEvent::AuthMigrationRecommended { .. } => "auth-migration-recommended".to_string(),
        DaemonEvent::Unknown => "unknown".to_string(),
    };

    bounded_event_label(label)
}

fn bounded_event_label(label: String) -> String {
    if label.chars().count() <= MAX_EVENT_LABEL_CHARS {
        return label;
    }

    let mut bounded = label
        .chars()
        .take(MAX_EVENT_LABEL_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn should_toast_playback_action(action: &str) -> bool {
    !matches!(action, "snapshot" | "synced" | "sync" | "poll") && !action.starts_with("optimistic-")
}

fn diagnostics_surface(cx: &App, title: &str, body: &str, lines: Vec<String>) -> impl IntoElement {
    let mut status_rows = div().mt_6().flex().flex_col();
    for line in lines {
        status_rows = status_rows.child(
            div()
                .mb_2()
                .text_sm()
                .text_color(rgb(cx.desktop_theme().text_secondary))
                .child(line),
        );
    }

    div()
        .size_full()
        .bg(rgb(cx.desktop_theme().bg_surface))
        .p_8()
        .flex()
        .flex_col()
        .items_start()
        .justify_start()
        .text_color(rgb(cx.desktop_theme().text_primary))
        .child(div().text_3xl().child(title.to_string()))
        .child(
            div()
                .mt_2()
                .text_lg()
                .text_color(rgb(cx.desktop_theme().text_secondary))
                .child(body.to_string()),
        )
        .child(status_rows)
}

fn health_word(is_healthy: bool) -> &'static str {
    if is_healthy {
        "healthy"
    } else {
        "down"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spotuify_core::{LyricLine, LyricsProvider, MediaKind, Playback};
    use spotuify_protocol::{UpgradeMethod, IPC_PROTOCOL_VERSION};

    #[cfg(debug_assertions)]
    #[test]
    fn debug_fps_reports_frames_over_a_half_second_sample() {
        let started_at = Instant::now();
        let mut fps = DebugFps {
            sample_started_at: started_at,
            sampled_frames: 0,
            frames_per_second: 0.,
            frame_time_ms: 0.,
        };

        for frame in 1..=32 {
            fps.record_frame(started_at + std::time::Duration::from_millis(frame * 16));
        }

        assert!((fps.frames_per_second - 62.5).abs() < 0.1);
        assert!((fps.frame_time_ms - 16.).abs() < 0.1);
    }

    #[test]
    fn sidebar_selection_updates_destination() {
        let mut app = DesktopApp::new();

        app.selected_destination = Destination::Albums;

        assert_eq!(app.selected_destination, Destination::Albums);
    }

    #[test]
    fn sidebar_has_no_queue_destination() {
        assert!(!Destination::ALL
            .iter()
            .any(|destination| destination.label() == "Queue"));
    }

    #[test]
    fn opening_queue_rail_requests_once_until_snapshot_arrives() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.toggle_queue_rail();
        app.toggle_queue_rail();
        app.toggle_queue_rail();

        assert!(app.queue_visible);
        assert!(app.queue_loading);
        assert!(app.queue_requested);
        assert!(matches!(command_rx.try_recv(), Ok(Request::QueueGet)));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn queue_snapshot_requests_each_distinct_row_artwork_once() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        let queue_item = |uri: &str, image_url: &str| MediaItem {
            uri: uri.to_string(),
            image_url_small: Some(image_url.to_string()),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };

        app.set_queue_seed(Queue {
            currently_playing: Some(queue_item(
                "spotify:track:current",
                "https://example.test/current.jpg",
            )),
            items: vec![
                queue_item("spotify:track:first", "https://example.test/first.jpg"),
                queue_item("spotify:track:duplicate", "https://example.test/first.jpg"),
            ],
            ..Queue::default()
        });

        let mut urls = Vec::new();
        while let Ok(Request::Image { url }) = command_rx.try_recv() {
            urls.push(url);
        }
        urls.sort();
        assert_eq!(
            urls,
            vec![
                "https://example.test/current.jpg".to_string(),
                "https://example.test/first.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn artwork_request_is_gated_by_url_and_cached_bytes() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playback = Some(Playback {
            item: Some(MediaItem {
                uri: "spotify:track:art".to_string(),
                image_url: Some("https://example.test/medium.jpg".to_string()),
                image_url_large: Some("https://example.test/hero.jpg".to_string()),
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            ..Playback::default()
        });

        app.request_artwork_for_current_track();
        app.request_artwork_for_current_track();
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::Image { url }) if url == "https://example.test/hero.jpg"
        ));
        assert!(command_rx.try_recv().is_err());

        app.apply_artwork_response(
            "https://example.test/hero.jpg".to_string(),
            ResponseData::Image {
                bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
        );
        assert!(app
            .artwork_cache
            .contains_key("https://example.test/hero.jpg"));
        app.request_artwork_for_current_track();
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn artwork_url_prefers_size_and_falls_back_to_default() {
        let item = MediaItem {
            image_url: Some("default".to_string()),
            image_url_small: Some("small".to_string()),
            image_url_large: Some("large".to_string()),
            ..MediaItem::default()
        };
        assert_eq!(artwork_url(&item, true).as_deref(), Some("large"));
        assert_eq!(artwork_url(&item, false).as_deref(), Some("small"));
        assert_eq!(
            artwork_url(
                &MediaItem {
                    image_url: Some("default".to_string()),
                    ..MediaItem::default()
                },
                true,
            )
            .as_deref(),
            Some("default")
        );
    }

    #[test]
    fn entering_liked_songs_requests_saved_tracks_once() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.select_destination(Destination::LikedSongs);
        app.select_destination(Destination::LikedSongs);

        assert!(app.liked_loading);
        assert!(app.liked_requested);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::SavedTracks {
                limit: 50,
                offset: 0,
                provider: None,
            })
        ));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn entering_albums_and_artists_requests_each_list_once() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.select_destination(Destination::Albums);
        app.select_destination(Destination::Albums);
        app.select_destination(Destination::Artists);
        app.select_destination(Destination::Artists);

        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LibraryList {
                limit: 100,
                provider: None
            })
        ));
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::FollowedArtists {
                limit: 100,
                provider: None
            })
        ));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn albums_and_artists_responses_update_their_state() {
        let mut app = DesktopApp::new();
        app.albums_loading = true;
        app.artists_loading = true;
        let album = MediaItem {
            name: "Album".to_string(),
            uri: "spotify:album:one".to_string(),
            kind: MediaKind::Album,
            ..MediaItem::default()
        };
        let track = MediaItem {
            name: "Not an album".to_string(),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };
        let artist = MediaItem {
            name: "Artist".to_string(),
            uri: "spotify:artist:one".to_string(),
            kind: MediaKind::Artist,
            ..MediaItem::default()
        };

        app.apply_albums_response(ResponseData::MediaItems {
            items: vec![album.clone(), track],
        });
        app.apply_artists_response(ResponseData::MediaItems {
            items: vec![artist.clone()],
        });

        assert_eq!(app.albums, vec![album]);
        assert_eq!(app.artists, vec![artist]);
        assert!(!app.albums_loading);
        assert!(!app.artists_loading);
        assert!(app.albums_requested);
        assert!(app.artists_requested);
    }

    #[test]
    fn album_and_artist_errors_clear_loading_and_allow_retry() {
        let mut app = DesktopApp::new();
        app.albums_requested = true;
        app.albums_loading = true;
        app.artists_requested = true;
        app.artists_loading = true;

        app.fail_albums("albums unavailable".to_string());
        app.fail_artists("artists unavailable".to_string());

        assert!(!app.albums_loading);
        assert!(!app.albums_requested);
        assert_eq!(app.albums_error.as_deref(), Some("albums unavailable"));
        assert!(!app.artists_loading);
        assert!(!app.artists_requested);
        assert_eq!(app.artists_error.as_deref(), Some("artists unavailable"));
    }

    #[test]
    fn library_change_refetches_loaded_albums_and_artists() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.albums_requested = true;
        app.artists_requested = true;

        app.apply_daemon_event(DaemonEvent::LibraryChanged {
            action: "saved".to_string(),
            uris: vec!["spotify:album:one".to_string()],
            provider: None,
        });

        assert!(app.albums_loading);
        assert!(app.artists_loading);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LibraryList {
                limit: 100,
                provider: None
            })
        ));
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::FollowedArtists {
                limit: 100,
                provider: None
            })
        ));
    }

    #[test]
    fn entering_history_requests_recently_played_once() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.select_destination(Destination::History);
        app.select_destination(Destination::History);

        assert!(app.history_loading);
        assert!(app.history_requested);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::RecentlyPlayed { .. })
        ));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn history_response_replaces_items_authoritatively() {
        let mut app = connected_app();
        app.history_loading = true;
        let item = MediaItem {
            name: "Recently played".to_string(),
            uri: "spotify:track:recent".to_string(),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };

        app.apply_history_response(ResponseData::MediaItems {
            items: vec![item.clone()],
        });

        assert_eq!(app.history, vec![item]);
        assert!(!app.history_loading);
        assert!(app.history_requested);
        assert_eq!(app.history_error, None);
    }

    #[test]
    fn history_error_clears_loading_and_allows_retry() {
        let mut app = DesktopApp::new();
        app.history_requested = true;
        app.history_loading = true;

        app.fail_history("daemon unavailable".to_string());

        assert!(!app.history_loading);
        assert!(!app.history_requested);
        assert_eq!(app.history_error.as_deref(), Some("daemon unavailable"));
    }

    #[test]
    fn history_refetches_after_sync_finished() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.history_requested = true;

        app.apply_daemon_event(DaemonEvent::SyncFinished {
            summary: spotuify_protocol::CacheSyncSummary {
                target: spotuify_protocol::SyncTargetData::Recent,
                playback_snapshots: 0,
                queue_snapshots: 0,
                queue_items: 0,
                devices: 0,
                playlists: 0,
                playlist_items: 0,
                recent_items: 1,
                library_items: 0,
                media_items: 0,
                provider: None,
                status: spotuify_protocol::SyncCompletionStatus::Succeeded,
                error: None,
                provider_outcomes: Vec::new(),
            },
        });

        assert!(app.history_loading);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::RecentlyPlayed { .. })
        ));
    }

    #[test]
    fn liked_songs_response_is_authoritative_and_library_change_refetches() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.select_destination(Destination::LikedSongs);
        let _ = command_rx.try_recv();

        let item = MediaItem {
            name: "Saved song".to_string(),
            uri: "spotify:track:saved".to_string(),
            kind: MediaKind::Track,
            image_url_small: Some("https://images.test/saved-small.jpg".to_string()),
            ..MediaItem::default()
        };
        app.apply_daemon_response(ResponseData::SavedTracksPage {
            items: vec![item.clone()],
            total: 101,
            offset: 0,
        });
        assert_eq!(app.liked_songs, vec![item]);
        assert_eq!(app.liked_total, 101);
        assert_eq!(app.liked_offset, 0);
        assert!(!app.liked_loading);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::Image { url }) if url == "https://images.test/saved-small.jpg"
        ));

        app.apply_daemon_event(DaemonEvent::LibraryChanged {
            action: "saved".to_string(),
            uris: vec!["spotify:track:saved".to_string()],
            provider: None,
        });
        assert!(app.liked_loading);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::SavedTracks {
                limit: 50,
                offset: 0,
                provider: None,
            })
        ));
    }

    fn test_playback(uri: &str, progress_ms: u64) -> Playback {
        Playback {
            item: Some(MediaItem {
                uri: uri.to_string(),
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            progress_ms,
            ..Playback::default()
        }
    }

    #[test]
    fn lyrics_request_is_gated_and_maps_response() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playback = Some(test_playback("spotify:track:one", 2_500));

        app.select_destination(Destination::Lyrics);
        app.select_destination(Destination::Lyrics);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LyricsGet { track_uri: Some(uri), force_refresh: false })
                if uri == "spotify:track:one"
        ));
        assert!(command_rx.try_recv().is_err());

        app.apply_daemon_response_for_track(
            ResponseData::Lyrics {
                lyrics: Some(SyncedLyrics {
                    provider: LyricsProvider::Lrclib,
                    track_uri: "spotify:track:one".to_string(),
                    lines: vec![LyricLine {
                        start_ms: 2_000,
                        text: "line".to_string(),
                        is_rtl: false,
                    }],
                    fetched_at_ms: 0,
                    synced: true,
                    language: None,
                    source_url: None,
                }),
                offset_ms: 500,
            },
            Some("spotify:track:one"),
        );
        assert_eq!(app.lyrics_offset_ms, 500);
        assert_eq!(app.lyrics.as_ref().unwrap().lines[0].text, "line");
        assert_eq!(
            active_lyric_line_index(&app.lyrics.as_ref().unwrap().lines, 2_500, 500),
            Some(0)
        );
    }

    #[test]
    fn lyrics_response_for_old_track_is_ignored() {
        let mut app = connected_app();
        let (command_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playback = Some(test_playback("spotify:track:one", 0));
        app.select_destination(Destination::Lyrics);
        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "next".to_string(),
            playback: Some(test_playback("spotify:track:two", 0)),
        });

        app.apply_daemon_response_for_track(
            ResponseData::Lyrics {
                lyrics: None,
                offset_ms: 0,
            },
            Some("spotify:track:one"),
        );
        assert_eq!(app.lyrics_track_uri.as_deref(), Some("spotify:track:two"));
        assert!(app.lyrics_loading);
    }

    #[test]
    fn queue_response_and_event_replace_snapshot_authoritatively() {
        let mut app = connected_app();
        let first = MediaItem {
            name: "First".to_string(),
            uri: "spotify:track:first".to_string(),
            ..MediaItem::default()
        };
        let second = MediaItem {
            name: "Second".to_string(),
            uri: "spotify:track:second".to_string(),
            ..MediaItem::default()
        };

        app.apply_daemon_response(ResponseData::Queue {
            queue: Queue {
                items: vec![first.clone()],
                ..Queue::default()
            },
        });
        assert_eq!(app.queue.as_ref().unwrap().items, vec![first]);
        assert!(!app.queue_loading);

        app.apply_daemon_event(DaemonEvent::QueueChanged {
            action: "synced".to_string(),
            uris: vec![second.uri.clone()],
            queue: Some(Queue {
                items: vec![second.clone()],
                ..Queue::default()
            }),
        });
        assert_eq!(app.queue.as_ref().unwrap().items, vec![second]);
    }

    #[test]
    fn client_seed_sets_queue_snapshot() {
        let mut app = DesktopApp::new();
        let item = MediaItem {
            name: "Seeded".to_string(),
            ..MediaItem::default()
        };

        app.set_queue_seed(Queue {
            currently_playing: Some(item.clone()),
            ..Queue::default()
        });

        assert_eq!(app.queue.as_ref().unwrap().currently_playing, Some(item));
        assert!(app.queue_requested);
        assert!(!app.queue_loading);
    }

    fn test_device(name: &str, active: bool) -> Device {
        Device {
            id: Some(format!("{name}-id")),
            name: name.to_string(),
            kind: "Computer".to_string(),
            is_active: active,
            is_restricted: false,
            volume_percent: Some(50),
            supports_volume: true,
        }
    }

    #[test]
    fn devices_seed_response_and_event_update_snapshot() {
        let mut app = DesktopApp::new();
        app.set_devices_seed(vec![test_device("seed", true)]);
        assert!(app.devices_loaded);
        assert_eq!(app.devices[0].name, "seed");

        app.apply_daemon_response(ResponseData::Devices {
            devices: vec![test_device("response", false)],
        });
        assert_eq!(app.devices[0].name, "response");
        assert!(!app.devices_loading);

        app.apply_daemon_event(DaemonEvent::DevicesChanged {
            action: "synced".to_string(),
            devices: Some(vec![test_device("event", false)]),
        });
        assert_eq!(app.devices[0].name, "event");
    }

    #[test]
    fn entering_devices_requests_once_until_snapshot_arrives() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.select_destination(Destination::Devices);
        app.select_destination(Destination::Devices);

        assert!(app.devices_loading);
        assert!(matches!(command_rx.try_recv(), Ok(Request::DevicesList)));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn device_transfer_uses_protocol_request() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.transfer_to_device("phone-id".to_string());
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::DeviceTransfer { device }) if device == "phone-id"
        ));
    }

    #[test]
    fn playback_event_updates_footer_state() {
        let mut app = connected_app();
        let playback = Playback {
            item: Some(MediaItem {
                name: "C.R.E.A.M.".to_string(),
                subtitle: "Wu-Tang Clan".to_string(),
                duration_ms: 238_000,
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            is_playing: true,
            progress_ms: 61_000,
            repeat: RepeatMode::Context,
            ..Playback::default()
        };

        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "optimistic-next".to_string(),
            playback: Some(playback),
        });

        let summary = playback_summary(app.playback.as_ref());
        assert_eq!(summary.title, "C.R.E.A.M.");
        assert_eq!(summary.progress, "1:01 / 3:58");
        assert_eq!(app.toast.as_deref(), None);
    }

    #[test]
    fn transport_state_round_trips_from_playback_event() {
        let mut app = connected_app();
        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "optimistic-shuffle".to_string(),
            playback: Some(Playback {
                shuffle: true,
                repeat: RepeatMode::Track,
                ..Playback::default()
            }),
        });

        let playback = app.playback.expect("playback event should seed state");
        assert!(playback.shuffle);
        assert_eq!(playback.repeat, RepeatMode::Track);
    }

    #[test]
    fn transport_controls_enqueue_protocol_mutations() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.send_playback_command(PlaybackCommand::Pause);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaybackCommand {
                command: PlaybackCommand::Pause
            })
        ));

        app.send_slider_command(PlaybackCommand::Seek {
            position_ms: 12_345,
        });
        app.send_slider_command(PlaybackCommand::Seek {
            position_ms: 98_765,
        });
        assert_eq!(
            slider_rx.borrow().clone(),
            Some(Request::PlaybackCommand {
                command: PlaybackCommand::Seek {
                    position_ms: 98_765,
                },
            })
        );
    }

    #[test]
    fn current_track_membership_is_requested_once_and_applied_authoritatively() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playback = Some(Playback {
            item: Some(MediaItem {
                uri: "spotify:track:already-saved".to_string(),
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            ..Playback::default()
        });

        app.request_current_track_membership();
        app.request_current_track_membership();
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LibraryContains { uris })
                if uris == vec!["spotify:track:already-saved"]
        ));
        assert!(command_rx.try_recv().is_err());
        app.toggle_current_track_like();
        assert!(command_rx.try_recv().is_err());

        app.apply_daemon_response(ResponseData::LibraryMembership {
            memberships: vec![spotuify_protocol::LibraryMembership {
                uri: "spotify:track:already-saved".to_string(),
                saved: true,
            }],
        });
        assert_eq!(app.current_track_like_status(), Some(true));
    }

    #[test]
    fn footer_like_toggles_from_daemon_owned_library_events() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playback = Some(Playback {
            item: Some(MediaItem {
                uri: "spotify:track:liked".to_string(),
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            ..Playback::default()
        });
        app.apply_daemon_response(ResponseData::LibraryMembership {
            memberships: vec![spotuify_protocol::LibraryMembership {
                uri: "spotify:track:liked".to_string(),
                saved: false,
            }],
        });

        app.toggle_current_track_like();
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LibrarySave {
                uri: Some(uri),
                current: false,
            }) if uri == "spotify:track:liked"
        ));
        assert_eq!(app.current_track_like_status(), Some(false));

        app.apply_daemon_event(DaemonEvent::LibraryChanged {
            action: "save".to_string(),
            uris: vec!["spotify:track:liked".to_string()],
            provider: None,
        });
        assert_eq!(app.current_track_like_status(), Some(true));

        app.toggle_current_track_like();
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::LibraryUnsave { uri }) if uri == "spotify:track:liked"
        ));
    }

    #[test]
    fn search_selection_extends_with_shift_and_preserves_anchor() {
        let (cursor, selected, anchor) = selection_after_cursor_move(11, None, 10, true, 11);
        assert_eq!(cursor, 10);
        assert_eq!(selected, 10..11);
        assert_eq!(anchor, Some(11));

        let (cursor, selected, anchor) = selection_after_cursor_move(10, anchor, 11, true, 11);
        assert_eq!(cursor, 11);
        assert!(selected.is_empty());
        assert_eq!(anchor, Some(11));
    }

    #[test]
    fn slider_click_fraction_clamps_to_bar_bounds() {
        let bounds = Bounds::from_corners(point(px(100.), px(0.)), point(px(300.), px(10.)));

        assert!((slider_fraction_at(point(px(200.), px(5.)), bounds) - 0.5).abs() < 0.001);
        assert_eq!(slider_fraction_at(point(px(50.), px(5.)), bounds), 0.0);
        assert_eq!(slider_fraction_at(point(px(350.), px(5.)), bounds), 1.0);
    }

    #[test]
    fn starting_search_enqueues_versioned_stream_request() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.search_query = "  radiohead  ".to_string();

        app.start_search();

        assert!(app.search_loading);
        assert_eq!(app.search_version, 1);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::SearchStream {
                query,
                scope: SearchScopeData::All,
                source: SearchSourceData::Remote(_),
                version: 1,
                provider: None,
            }) if query == "radiohead"
        ));
    }

    #[test]
    fn search_events_ignore_stale_versions_and_complete_current_query() {
        let mut app = connected_app();
        app.search_query = "radiohead".to_string();
        app.search_version = 2;
        let item = MediaItem {
            name: "Everything In Its Right Place".to_string(),
            uri: "spotify:track:1".to_string(),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };

        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: "radiohead".to_string(),
            kind: MediaKind::Track,
            offset: 0,
            version: 1,
            items: vec![item.clone()],
            provider: None,
        });
        assert!(app.search_results.is_empty());

        app.search_loading = true;
        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: "radiohead".to_string(),
            kind: MediaKind::Track,
            offset: 0,
            version: 2,
            items: vec![item],
            provider: None,
        });
        assert_eq!(app.search_results.len(), 1);

        app.apply_daemon_event(DaemonEvent::SearchComplete {
            query: "radiohead".to_string(),
            version: 2,
            provider: None,
        });
        assert!(!app.search_loading);
    }

    #[test]
    fn search_stream_keeps_partial_results_visible_until_complete() {
        let mut app = connected_app();
        app.search_query = "action bronson".to_string();
        app.search_version = 1;
        app.search_loading = true;

        app.apply_daemon_event(DaemonEvent::SearchFailed {
            query: app.search_query.clone(),
            version: 1,
            kind: Some(MediaKind::Playlist),
            offset: Some(0),
            message: "playlist page failed".to_string(),
            provider: None,
        });
        assert!(app.search_loading);
        assert_eq!(app.search_error.as_deref(), Some("playlist page failed"));

        let show = MediaItem {
            name: "A show".to_string(),
            kind: MediaKind::Show,
            ..MediaItem::default()
        };
        let track = MediaItem {
            name: "A track".to_string(),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };
        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: app.search_query.clone(),
            kind: MediaKind::Show,
            offset: 0,
            version: 1,
            items: vec![show],
            provider: None,
        });
        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: app.search_query.clone(),
            kind: MediaKind::Track,
            offset: 0,
            version: 1,
            items: vec![track],
            provider: None,
        });

        assert_eq!(app.search_results[0].kind, MediaKind::Track);
        assert!(app.search_loading);
        app.apply_daemon_event(DaemonEvent::SearchComplete {
            query: app.search_query.clone(),
            version: 1,
            provider: None,
        });
        assert!(!app.search_loading);
    }

    #[test]
    fn playlist_response_completes_picker_loading_state() {
        let mut app = DesktopApp::new();
        app.playlist_picker_uri = Some("spotify:track:1".to_string());
        app.playlist_loading = true;

        app.apply_daemon_response(ResponseData::Playlists {
            playlists: vec![Playlist {
                id: "playlist-1".to_string(),
                name: "Favorites".to_string(),
                owner: "me".to_string(),
                tracks_total: 1,
                image_url: None,
                version_token: None,
            }],
        });

        assert!(!app.playlist_loading);
        assert_eq!(app.search_playlists[0].name, "Favorites");
    }

    #[test]
    fn entering_playlists_requests_once_until_response_arrives() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.select_destination(Destination::Playlists);
        app.select_destination(Destination::Playlists);

        assert!(app.playlists_loading);
        assert!(app.playlists_requested);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaylistsList { .. })
        ));
        assert!(command_rx.try_recv().is_err());
    }

    #[test]
    fn playlists_response_populates_list_state() {
        let mut app = DesktopApp::new();
        app.playlists_loading = true;
        app.apply_daemon_response(ResponseData::Playlists {
            playlists: vec![Playlist {
                id: "playlist-1".to_string(),
                name: "Favorites".to_string(),
                owner: "me".to_string(),
                tracks_total: 2,
                image_url: None,
                version_token: None,
            }],
        });

        assert_eq!(app.playlists.len(), 1);
        assert_eq!(app.playlists[0].name, "Favorites");
        assert!(!app.playlists_loading);
        assert!(app.playlists_requested);
    }

    #[test]
    fn playlist_tracks_response_populates_selected_detail() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.open_playlist(Playlist {
            id: "playlist-1".to_string(),
            name: "Favorites".to_string(),
            owner: "me".to_string(),
            tracks_total: 1,
            image_url: None,
            version_token: None,
        });
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaylistTracks { playlist, wait: false, provider: None }) if playlist == "playlist-1"
        ));

        app.apply_playlist_tracks_response(
            "playlist-1",
            ResponseData::MediaItems {
                items: vec![MediaItem {
                    name: "Song".to_string(),
                    uri: "spotify:track:song".to_string(),
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                }],
            },
        );

        assert_eq!(app.playlist_tracks.len(), 1);
        assert_eq!(app.playlist_tracks[0].name, "Song");
        assert!(!app.playlist_tracks_loading);
    }

    #[test]
    fn album_detail_requests_once_and_applies_matching_response() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        let album = MediaItem {
            uri: "spotify:album:album-1".to_string(),
            name: "Album".to_string(),
            kind: MediaKind::Album,
            ..MediaItem::default()
        };

        app.open_album(album.clone());
        app.open_album(album);

        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::AlbumTracks { album }) if album == "spotify:album:album-1"
        ));
        assert!(command_rx.try_recv().is_err());
        app.apply_album_tracks_response(
            "spotify:album:album-1",
            ResponseData::MediaItems {
                items: vec![MediaItem {
                    uri: "spotify:track:track-1".to_string(),
                    name: "Track".to_string(),
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                }],
            },
        );
        assert_eq!(app.album_tracks.len(), 1);
        assert!(!app.album_tracks_loading);
    }

    #[test]
    fn catalog_navigation_links_preserve_nested_detail_history() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.selected_destination = Destination::Search;
        let album = MediaItem {
            uri: "spotify:album:album-1".to_string(),
            name: "Album".to_string(),
            kind: MediaKind::Album,
            ..MediaItem::default()
        };
        let artist = MediaItem {
            uri: "spotify:artist:artist-1".to_string(),
            name: "Artist".to_string(),
            kind: MediaKind::Artist,
            ..MediaItem::default()
        };

        app.navigate_to_album(album.clone());
        assert_eq!(app.selected_destination, Destination::Albums);
        assert_eq!(app.selected_album.as_ref(), Some(&album));
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::AlbumTracks { album }) if album == "spotify:album:album-1"
        ));

        app.navigate_to_artist(artist.clone());
        assert_eq!(app.selected_destination, Destination::Artists);
        assert_eq!(app.selected_artist.as_ref(), Some(&artist));
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::ArtistAlbums { artist }) if artist == "spotify:artist:artist-1"
        ));

        app.navigate_back_from_detail();
        assert_eq!(app.selected_destination, Destination::Albums);
        assert_eq!(app.selected_album.as_ref(), Some(&album));
        app.navigate_back_from_detail();
        assert_eq!(app.selected_destination, Destination::Search);
        assert!(app.selected_album.is_none());
    }

    #[test]
    fn artist_detail_response_and_error_are_scoped_to_selected_artist() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.open_artist(MediaItem {
            uri: "spotify:artist:artist-1".to_string(),
            name: "Artist".to_string(),
            kind: MediaKind::Artist,
            ..MediaItem::default()
        });
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::ArtistAlbums { artist }) if artist == "spotify:artist:artist-1"
        ));

        app.apply_artist_albums_response(
            "spotify:artist:other",
            ResponseData::MediaItems { items: vec![] },
        );
        assert!(app.artist_albums_loading);
        app.fail_artist_albums("spotify:artist:artist-1", "unavailable".to_string());
        assert!(!app.artist_albums_loading);
        assert_eq!(app.artist_albums_error.as_deref(), Some("unavailable"));
    }

    #[test]
    fn detail_back_navigation_clears_album_and_artist_state() {
        let mut app = DesktopApp::new();
        app.selected_album = Some(MediaItem {
            uri: "spotify:album:1".to_string(),
            ..MediaItem::default()
        });
        app.album_tracks.push(MediaItem::default());
        app.close_album();
        assert!(app.selected_album.is_none());
        assert!(app.album_tracks.is_empty());

        app.selected_artist = Some(MediaItem {
            uri: "spotify:artist:1".to_string(),
            ..MediaItem::default()
        });
        app.artist_albums.push(MediaItem::default());
        app.close_artist();
        assert!(app.selected_artist.is_none());
        assert!(app.artist_albums.is_empty());
    }

    #[test]
    fn loaded_playlists_refetch_on_change_event() {
        let mut app = connected_app();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playlists_requested = true;
        app.playlists_loading = false;

        app.apply_daemon_event(DaemonEvent::PlaylistsChanged {
            action: "updated".to_string(),
            playlist: Some("playlist-1".to_string()),
            provider: None,
        });

        assert!(app.playlists_loading);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaylistsList { .. })
        ));
    }

    #[test]
    fn selecting_playlist_enqueues_add_request_and_closes_picker() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playlist_picker_uri = Some("spotify:track:1".to_string());

        app.add_search_result_to_playlist("playlist-1".to_string());

        assert_eq!(app.playlist_picker_uri, None);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaylistAddItems { playlist, uris, .. })
                if playlist == "playlist-1" && uris == vec!["spotify:track:1"]
        ));
    }

    #[test]
    fn slider_preview_is_local_until_release_then_commits_latest_value() {
        let mut app = DesktopApp::new();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);

        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(98_765));
        assert_eq!(
            app.slider_preview,
            Some(SliderPreview::Seek(98_765)),
            "drag feedback should be available before the daemon replies"
        );
        assert_eq!(*slider_rx.borrow(), None);

        app.finish_slider_drag(SliderKind::Seek);

        assert_eq!(
            app.slider_preview,
            Some(SliderPreview::Seek(98_765)),
            "release keeps the optimistic position visible until playback catches up"
        );
        assert_eq!(
            *slider_rx.borrow(),
            Some(Request::PlaybackCommand {
                command: PlaybackCommand::Seek {
                    position_ms: 98_765,
                },
            })
        );
    }

    #[test]
    fn seek_release_does_not_flash_a_stale_playback_snapshot() {
        let mut app = DesktopApp::new();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);
        app.playback = Some(Playback {
            item: Some(MediaItem {
                uri: "spotify:track:test".to_string(),
                duration_ms: 180_000,
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            is_playing: true,
            progress_ms: 10_000,
            ..Playback::default()
        });

        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(60_000));
        app.finish_slider_drag(SliderKind::Seek);

        assert_eq!(
            app.slider_preview,
            Some(SliderPreview::Seek(60_000)),
            "the dropped position must remain rendered until daemon reconciliation"
        );

        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "snapshot".to_string(),
            playback: Some(Playback {
                item: Some(MediaItem {
                    uri: "spotify:track:test".to_string(),
                    duration_ms: 180_000,
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                }),
                is_playing: true,
                progress_ms: 10_200,
                ..Playback::default()
            }),
        });
        assert_eq!(
            app.slider_preview,
            Some(SliderPreview::Seek(60_000)),
            "a stale snapshot must not clear the pending seek preview"
        );

        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "optimistic-seek".to_string(),
            playback: Some(Playback {
                item: Some(MediaItem {
                    uri: "spotify:track:test".to_string(),
                    duration_ms: 180_000,
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                }),
                is_playing: true,
                progress_ms: 60_000,
                ..Playback::default()
            }),
        });
        assert_eq!(app.slider_preview, None);
    }

    #[test]
    fn failed_slider_mutation_releases_pending_preview() {
        let mut app = DesktopApp::new();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);
        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(60_000));
        app.finish_slider_drag(SliderKind::Seek);

        let receipt_id = ReceiptId::new_v7();
        app.apply_daemon_event(DaemonEvent::MutationAccepted {
            receipt_id,
            action: "seek".to_string(),
        });
        assert_eq!(app.slider_pending_receipt, Some(receipt_id));

        app.apply_daemon_event(DaemonEvent::MutationFinalized {
            receipt_id,
            status: spotuify_protocol::ReceiptStatus::Failed,
            message: "seek failed".to_string(),
        });
        assert_eq!(app.slider_preview, None);
        assert_eq!(app.slider_pending_receipt, None);
    }

    #[test]
    fn releasing_over_another_slider_does_not_abort_the_active_preview() {
        let mut app = DesktopApp::new();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);

        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(12_345));
        app.finish_slider_drag(SliderKind::Volume);

        assert_eq!(app.slider_preview, Some(SliderPreview::Seek(12_345)));
        assert_eq!(*slider_rx.borrow(), None);
    }

    #[test]
    fn transport_value_mapping_clamps_and_rounds() {
        assert_eq!(seek_position_ms(200_000, 0.25), 50_000);
        assert_eq!(seek_position_ms(200_000, -1.0), 0);
        assert_eq!(seek_position_ms(200_000, 2.0), 200_000);
        assert_eq!(volume_percent(0.505), 51);
        assert_eq!(volume_percent(-1.0), 0);
        assert_eq!(volume_percent(2.0), 100);
    }

    #[test]
    fn playing_progress_is_derived_from_daemon_sample_time() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_millis() as i64;
        let playback = Playback {
            item: Some(MediaItem {
                duration_ms: 10_000,
                ..MediaItem::default()
            }),
            is_playing: true,
            progress_ms: 1_000,
            sampled_at_ms: Some(now_ms - 2_000),
            ..Playback::default()
        };

        assert!(playback_progress_ms(&playback) >= 2_900);
        assert!(playback_progress_ms(&playback) <= 10_000);
    }

    #[test]
    fn repeat_button_cycles_through_daemon_modes() {
        assert_eq!(next_repeat_state(RepeatMode::Off), RepeatMode::Context);
        assert_eq!(next_repeat_state(RepeatMode::Context), RepeatMode::Track);
        assert_eq!(next_repeat_state(RepeatMode::Track), RepeatMode::Off);
    }

    #[test]
    fn update_event_sets_banner() {
        let mut app = connected_app();

        app.apply_daemon_event(DaemonEvent::UpdateAvailable {
            latest_version: "0.1.79".to_string(),
            release_url: None,
            upgrade: UpgradeHint {
                method: UpgradeMethod::Homebrew,
                command: Some("brew upgrade spotuify".to_string()),
                url: Some("https://example.test/release".to_string()),
            },
        });

        assert_eq!(
            app.update_banner
                .as_ref()
                .map(|banner| banner.latest_version.as_str()),
            Some("0.1.79")
        );
        assert_eq!(
            app.update_banner
                .as_ref()
                .and_then(|banner| banner.command.as_deref()),
            Some("brew upgrade spotuify")
        );
        assert_eq!(app.toast.as_deref(), Some("Update available: 0.1.79"));
    }

    #[test]
    fn diagnostic_event_label_does_not_render_full_sync_payload() {
        let label = event_label(&DaemonEvent::SyncFinished {
            summary: spotuify_protocol::CacheSyncSummary {
                target: spotuify_protocol::SyncTargetData::Queue,
                playback_snapshots: 0,
                queue_snapshots: 0,
                queue_items: 20,
                devices: 0,
                playlists: 0,
                playlist_items: 0,
                recent_items: 0,
                library_items: 0,
                media_items: 0,
                provider: None,
                status: spotuify_protocol::SyncCompletionStatus::Succeeded,
                error: None,
                provider_outcomes: Vec::new(),
            },
        });

        assert_eq!(label, "sync-finished:queue");
        assert!(label.chars().count() <= MAX_EVENT_LABEL_CHARS);
    }

    fn connected_app() -> DesktopApp {
        let mut app = DesktopApp::new();
        app.state = DesktopState::Connected(ConnectedState {
            daemon_status: DaemonStatus {
                running: true,
                socket_path: "test.sock".to_string(),
                socket_exists: true,
                socket_reachable: true,
                stale_socket: false,
                daemon_pid: None,
                uptime_secs: None,
                protocol_version: IPC_PROTOCOL_VERSION,
                daemon_version: Some("test".to_string()),
                daemon_build_id: None,
                audio_health: None,
            },
            doctor_report: None,
            last_event: None,
        });
        app
    }
}

#[cfg(all(test, feature = "test-support"))]
mod gpui_tests {
    use super::*;
    use gpui::{point, px, size, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext};
    use spotuify_protocol::IPC_PROTOCOL_VERSION;

    fn initialize_theme(cx: &mut TestAppContext) {
        cx.set_global(theme::DesktopTheme::new(gpui::WindowAppearance::Light));
    }

    #[gpui::test]
    fn search_results_scroll_with_mouse_wheel(cx: &mut TestAppContext) {
        initialize_theme(cx);
        let (view, cx) = cx.add_window_view(|_, _| DesktopApp::new());
        view.update(cx, |app, cx| {
            app.state = DesktopState::Connected(ConnectedState {
                daemon_status: DaemonStatus {
                    running: true,
                    socket_path: "test.sock".to_string(),
                    socket_exists: true,
                    socket_reachable: true,
                    stale_socket: false,
                    daemon_pid: None,
                    uptime_secs: None,
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("test".to_string()),
                    daemon_build_id: None,
                    audio_health: None,
                },
                doctor_report: None,
                last_event: None,
            });
            app.selected_destination = Destination::Search;
            app.search_query = "action bronson".to_string();
            app.search_results = (0..40)
                .map(|index| MediaItem {
                    name: format!("Result {index}"),
                    uri: spotuify_core::ResourceUri::spotify(MediaKind::Track, format!("{index}"))
                        .expect("test track uri is valid")
                        .as_uri(),
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                })
                .collect();
            cx.notify();
        });
        cx.simulate_resize(size(px(900.), px(650.)));
        cx.run_until_parked();

        let scroll = cx.read(|app| view.read(app).search_scroll.clone());
        assert!(
            scroll.max_offset().height > px(0.),
            "search result list should have overflow after layout"
        );
        let bounds = scroll.bounds();
        cx.simulate_event(ScrollWheelEvent {
            position: bounds.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-500.))),
            ..Default::default()
        });
        cx.run_until_parked();

        assert!(
            scroll.offset().y < px(0.),
            "search result list should move in response to the mouse wheel"
        );
    }

    #[gpui::test]
    fn liked_songs_scroll_with_mouse_wheel(cx: &mut TestAppContext) {
        initialize_theme(cx);
        let (view, cx) = cx.add_window_view(|_, _| DesktopApp::new());
        view.update(cx, |app, cx| {
            app.state = DesktopState::Connected(ConnectedState {
                daemon_status: DaemonStatus {
                    running: true,
                    socket_path: "test.sock".to_string(),
                    socket_exists: true,
                    socket_reachable: true,
                    stale_socket: false,
                    daemon_pid: None,
                    uptime_secs: None,
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("test".to_string()),
                    daemon_build_id: None,
                    audio_health: None,
                },
                doctor_report: None,
                last_event: None,
            });
            app.selected_destination = Destination::LikedSongs;
            app.liked_total = 40;
            app.liked_songs = (0..40)
                .map(|index| MediaItem {
                    name: format!("Liked track {index}"),
                    uri: spotuify_core::ResourceUri::spotify(MediaKind::Track, format!("{index}"))
                        .expect("test track uri is valid")
                        .as_uri(),
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                })
                .collect();
            cx.notify();
        });
        cx.simulate_resize(size(px(900.), px(650.)));
        cx.run_until_parked();

        let scroll = cx.read(|app| view.read(app).liked_songs_scroll.clone());
        assert!(
            scroll.max_offset().height > px(0.),
            "liked songs should have overflow after layout"
        );
        let bounds = scroll.bounds();
        cx.simulate_event(ScrollWheelEvent {
            position: bounds.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-500.))),
            ..Default::default()
        });
        cx.run_until_parked();

        assert!(
            scroll.offset().y < px(0.),
            "liked songs should move in response to the mouse wheel"
        );
    }

    #[gpui::test]
    fn footer_artist_link_opens_artist_detail(cx: &mut TestAppContext) {
        initialize_theme(cx);
        let (view, cx) = cx.add_window_view(|_, _| DesktopApp::new());
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        view.update(cx, |app, _| {
            app.state = DesktopState::Connected(ConnectedState {
                daemon_status: DaemonStatus {
                    running: true,
                    socket_path: "test.sock".to_string(),
                    socket_exists: true,
                    socket_reachable: true,
                    stale_socket: false,
                    daemon_pid: None,
                    uptime_secs: None,
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("test".to_string()),
                    daemon_build_id: None,
                    audio_health: None,
                },
                doctor_report: None,
                last_event: None,
            });
            app.set_command_senders(command_tx, slider_tx);
            app.playback = Some(Playback {
                item: Some(MediaItem {
                    name: "Track".to_string(),
                    uri: "spotify:track:track".to_string(),
                    kind: MediaKind::Track,
                    artists: vec![spotuify_core::ArtistRef {
                        name: "Linked Artist".to_string(),
                        uri: "spotify:artist:linked".to_string(),
                    }],
                    ..MediaItem::default()
                }),
                ..Playback::default()
            });
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
        cx.run_until_parked();

        let bounds = cx
            .debug_bounds("footer-current-track-artist-0")
            .expect("footer artist link should be rendered");
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.run_until_parked();

        cx.read(|app| {
            let app = view.read(app);
            assert_eq!(app.selected_destination, Destination::Artists);
            assert_eq!(
                app.selected_artist
                    .as_ref()
                    .map(|artist| artist.uri.as_str()),
                Some("spotify:artist:linked")
            );
        });
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::ArtistAlbums { artist }) if artist == "spotify:artist:linked"
        ));
    }

    #[gpui::test]
    fn seek_bar_accepts_click_without_drag(cx: &mut TestAppContext) {
        initialize_theme(cx);
        let (view, cx) = cx.add_window_view(|_, _| DesktopApp::new());
        let (command_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        view.update(cx, |app, _| {
            app.state = DesktopState::Connected(ConnectedState {
                daemon_status: DaemonStatus {
                    running: true,
                    socket_path: "test.sock".to_string(),
                    socket_exists: true,
                    socket_reachable: true,
                    stale_socket: false,
                    daemon_pid: None,
                    uptime_secs: None,
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("test".to_string()),
                    daemon_build_id: None,
                    audio_health: None,
                },
                doctor_report: None,
                last_event: None,
            });
            app.playback = Some(Playback {
                item: Some(MediaItem {
                    name: "Test track".to_string(),
                    uri: "spotify:track:test".to_string(),
                    duration_ms: 100_000,
                    kind: MediaKind::Track,
                    ..MediaItem::default()
                }),
                ..Playback::default()
            });
            app.set_command_senders(command_tx, slider_tx);
        });
        cx.simulate_resize(size(px(1_200.), px(700.)));
        cx.run_until_parked();

        let bounds = cx
            .debug_bounds("seek-bar")
            .expect("seek bar should be laid out");
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.run_until_parked();

        assert_eq!(
            slider_rx.borrow().clone(),
            Some(Request::PlaybackCommand {
                command: PlaybackCommand::Seek {
                    position_ms: 50_000,
                },
            })
        );
    }

    #[gpui::test]
    fn search_input_supports_shift_selection(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| DesktopApp::new());
        view.update(cx, |app, _| {
            app.state = DesktopState::Connected(ConnectedState {
                daemon_status: DaemonStatus {
                    running: true,
                    socket_path: "test.sock".to_string(),
                    socket_exists: true,
                    socket_reachable: true,
                    stale_socket: false,
                    daemon_pid: None,
                    uptime_secs: None,
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("test".to_string()),
                    daemon_build_id: None,
                    audio_health: None,
                },
                doctor_report: None,
                last_event: None,
            });
            app.selected_destination = Destination::Search;
        });
        cx.simulate_resize(size(px(1_200.), px(700.)));
        cx.run_until_parked();

        let bounds = cx
            .debug_bounds("search-input")
            .expect("search input should be laid out");
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.simulate_input("abc");
        cx.simulate_keystrokes("shift-left");

        let (content, selected_range, cursor) = cx.read(|app| {
            let input = view
                .read(app)
                .search_input
                .clone()
                .expect("search input should be initialized");
            let input = input.read(app);
            (
                input.content.clone(),
                input.selected_range.clone(),
                input.cursor,
            )
        });
        assert_eq!(content.as_ref(), "abc");
        assert_eq!(selected_range, 2..3);
        assert_eq!(cursor, 2);
    }
}
