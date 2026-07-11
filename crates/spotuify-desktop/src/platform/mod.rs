pub mod macos;

use gpui::AppContext;
use gpui::{Application, WindowOptions};

use crate::views::PlaceholderView;

pub fn run() {
    let message = macos::placeholder_message();

    Application::new().run(move |app| {
        let _ = app.open_window(WindowOptions::default(), move |_, cx| {
            cx.new(move |_| PlaceholderView::new(message))
        });
    });
}
