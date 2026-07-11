use gpui::prelude::*;
use gpui::{div, Window};
use spotuify_protocol::IPC_PROTOCOL_VERSION;

pub struct PlaceholderView {
    message: String,
}

impl PlaceholderView {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl Render for PlaceholderView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(format!("{} · IPC v{}", self.message, IPC_PROTOCOL_VERSION))
    }
}
