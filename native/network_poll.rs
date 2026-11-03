fn hook(value: Vec<Event>, session: &Session) -> Vec<Event> {
    crate::net::flashback_net::poll(session, value)
}
