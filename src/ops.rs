use crate::jj::BookmarkPresence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seed {
    Revset(String), // seed -r <revset>, no fetch (local bookmark, or `name@git`)
    Fetch(String),  // real-remote only: fetch -b <name>, then -r <name>
    Fresh,          // nothing matches → no -r
}

/// Pure: choose the seed strategy. Precedence:
/// local jj bookmark → colocated git branch → real remote → fresh.
pub fn decide_seed(name: &str, p: &BookmarkPresence) -> Seed {
    if p.local {
        Seed::Revset(name.to_string())
    } else if p.git {
        Seed::Revset(format!("{name}@git"))
    } else if p.remote.is_some() {
        Seed::Fetch(name.to_string())
    } else {
        Seed::Fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: fn(&str, &BookmarkPresence) -> Seed = decide_seed;

    #[test]
    fn seed_precedence() {
        let local = BookmarkPresence {
            local: true,
            git: true,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &local), Seed::Revset("x".into()));
        let git = BookmarkPresence {
            local: false,
            git: true,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &git), Seed::Revset("x@git".into()));
        let remote = BookmarkPresence {
            local: false,
            git: false,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &remote), Seed::Fetch("x".into()));
        assert_eq!(decide_seed("x", &BookmarkPresence::default()), Seed::Fresh);
    }
}
