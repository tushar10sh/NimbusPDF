pub mod local;

// LocalStorage is the concrete implementation used throughout the codebase.
// Routes import it as `crate::storage::local::LocalStorage`.

#[derive(Clone, Debug)]
pub enum Principal {
    Anonymous(String), // session id
    User(String),      // OIDC subject
}

impl Principal {
    pub fn from_session(session: &crate::session::SessionData) -> Self {
        match &session.user {
            Some(u) => Principal::User(u.subject.clone()),
            None => Principal::Anonymous(session.session_id.clone()),
        }
    }
}
