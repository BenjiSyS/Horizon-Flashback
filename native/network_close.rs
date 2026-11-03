fn hook(session: &mut Session) {
    crate::net::flashback_net::disconnect(session);
}
