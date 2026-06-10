//! `viz` request handlers (split out of the dispatch god-function).

#![allow(unused_imports)]
use std::sync::Arc;
use std::time::{Duration, Instant};

use spotuify_core::{now_ms, search_performed_event, Playback};
use spotuify_protocol::{
    CommandReceipt, DaemonEvent, EpisodeSort, Operation, OperationId, OperationKind,
    OperationSource, OperationStatus, PlaybackCommand, PlaylistCreateReceipt, ReceiptId, Request,
    Response, ResponseData, SearchScopeData, SearchSortData, SearchSourceData,
};
use spotuify_spotify::actions::{self, CommandKind};
use spotuify_spotify::client::{MediaItem, MediaKind, SpotifyClient};
use spotuify_spotify::config::Config;
use spotuify_spotify::selection;

use crate::analytics::AnalyticsStore;
use crate::handler::*;
use crate::retention::retention_cutoffs;
use crate::state::{DaemonState, FastTransportStatus};

pub(crate) async fn dispatch(
    state: Arc<DaemonState>,
    request: Request,
    _source: Option<OperationSource>,
) -> anyhow::Result<ResponseData> {
    match request {
        Request::SetVizEnabled { enabled } => {
            state.viz_coordinator().set_enabled(enabled).await;
            Ok(ResponseData::Ack {
                message: format!(
                    "visualization {}",
                    if enabled { "enabled" } else { "disabled" }
                ),
            })
        }
        Request::SetVizSource { kind } => {
            state.viz_coordinator().set_source(kind).await;
            Ok(ResponseData::Ack {
                message: format!("visualization source set to {}", kind.as_str()),
            })
        }
        Request::GetVizStatus => Ok(ResponseData::VizStatus {
            diagnostics: state.viz_coordinator().diagnostics().await,
        }),
        Request::SetVizFocus { focused } => {
            state.viz_coordinator().set_focused(focused).await;
            Ok(ResponseData::Ack {
                message: format!("viz focus = {focused}"),
            })
        }
        _ => unreachable!("non-viz request routed to viz dispatcher"),
    }
}
