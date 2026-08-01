use spotuify_launcher::no_daemon_start;
use spotuify_protocol::IPC_PROTOCOL_VERSION;

pub fn placeholder_message() -> String {
    let mut message = format!(
        "connecting... {} backend, IPC v{}",
        "embedded",
        IPC_PROTOCOL_VERSION
    );

    if no_daemon_start() {
        message.push_str(" (daemon auto-start disabled)");
    }

    message
}
