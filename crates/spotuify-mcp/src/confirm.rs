//! Destructive-action confirmation gating.
//!
//! MCP tools marked `destructive: true` in the [`crate::tools`] catalogue
//! must be invoked with `confirm: true` in their args. Without it, the
//! bridge returns a [`ConfirmationRequired`] error directing the LLM to
//! ask the user.
//!
//! Pattern mirrors spotify-player commit #966 (confirmation popups on
//! destructive actions) at the MCP layer.

use crate::tools::ToolCatalogue;

#[derive(Debug, thiserror::Error)]
pub enum ConfirmDecision {
    #[error("tool `{0}` not found in catalogue")]
    UnknownTool(String),
    #[error(
        "tool `{tool}` is destructive and requires `confirm: true` in args. \
         Without it, the call returns a preview. Ask the user to confirm before retrying."
    )]
    ConfirmationRequired { tool: String },
    #[error("tool `{tool}` doesn't accept a `confirm` arg")]
    UnexpectedConfirm { tool: String },
}

/// Raised by `decide()` when a destructive call lacks `confirm: true`.
pub type ConfirmationRequired = ConfirmDecision;

/// Decide whether a tool call is authorized to execute.
///
/// Pure function:
/// - Read/Transport/Mercury/Analytics/Ops tools: always allowed.
/// - Destructive tools: require `confirm == Some(true)`. `Some(false)` or
///   `None` returns ConfirmationRequired so the bridge returns a preview.
///
/// `undo_last` is exempt -- it IS the safety net, so requiring confirm
/// to undo would defeat the point.
pub fn decide(tool_name: &str, confirm: Option<bool>) -> Result<Authorized, ConfirmDecision> {
    let tool = ToolCatalogue::by_name(tool_name)
        .ok_or_else(|| ConfirmDecision::UnknownTool(tool_name.to_string()))?;

    if !tool.destructive {
        if confirm.is_some() {
            // Confirm sent on a non-destructive tool -- harmless; allow.
            return Ok(Authorized::Execute);
        }
        return Ok(Authorized::Execute);
    }

    match confirm {
        Some(true) => Ok(Authorized::Execute),
        _ => Ok(Authorized::PreviewOnly),
    }
}

/// What the caller should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorized {
    /// Run the tool and return its receipt/result.
    Execute,
    /// Build and return a preview instead of executing. The MCP response
    /// includes the preview so the LLM can show it to the user.
    PreviewOnly,
}
