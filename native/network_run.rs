fn hook(
    address: &str,
    writer: &Mutex<Option<ConnWriter>>,
    send: &Sender<Event>,
) -> core::ops::ControlFlow<Result<Reason, Reason>> {
    match crate::net::flashback_net::run(address, writer, send) {
        Some(result) => core::ops::ControlFlow::Break(result),
        None => core::ops::ControlFlow::Continue(()),
    }
}
