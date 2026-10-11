//! Shared route identities for the local metadata protocol.

pub(crate) const LOGS_PATH: &str = "/v1/logs";
pub(crate) const TRACES_PATH: &str = "/v1/traces";
pub(crate) const CODEX_HOOK_PATH: &str = "/v1/hooks/codex";
pub(crate) const CLAUDE_HOOK_PATH: &str = "/v1/hooks/claude";
pub(crate) const ACTIVITY_PATH: &str = "/v1/activity";
pub(crate) const SNAPSHOT_PATH: &str = "/v1/snapshot";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Route {
    Logs,
    Traces,
    CodexHook,
    ClaudeHook,
    Activity,
    Snapshot,
}

impl Route {
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            LOGS_PATH => Some(Self::Logs),
            TRACES_PATH => Some(Self::Traces),
            CODEX_HOOK_PATH => Some(Self::CodexHook),
            CLAUDE_HOOK_PATH => Some(Self::ClaudeHook),
            ACTIVITY_PATH => Some(Self::Activity),
            SNAPSHOT_PATH => Some(Self::Snapshot),
            _ => None,
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::Logs => LOGS_PATH,
            Self::Traces => TRACES_PATH,
            Self::CodexHook => CODEX_HOOK_PATH,
            Self::ClaudeHook => CLAUDE_HOOK_PATH,
            Self::Activity => ACTIVITY_PATH,
            Self::Snapshot => SNAPSHOT_PATH,
        }
    }

    /// OTLP acknowledgements count rejected records using the signal's wire field.
    pub fn rejected_record_field(self) -> Option<&'static str> {
        match self {
            Self::Logs => Some("rejectedLogRecords"),
            Self::Traces => Some("rejectedSpans"),
            Self::CodexHook | Self::ClaudeHook | Self::Activity | Self::Snapshot => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_round_trip_only_exact_protocol_paths() {
        for route in [
            Route::Logs,
            Route::Traces,
            Route::CodexHook,
            Route::ClaudeHook,
            Route::Activity,
            Route::Snapshot,
        ] {
            assert_eq!(Route::from_path(route.path()), Some(route));
            for changed in [
                format!("{}?token=private", route.path()),
                format!("{}#fragment", route.path()),
                format!("{}/", route.path()),
                route.path().to_ascii_uppercase(),
            ] {
                assert_eq!(Route::from_path(&changed), None);
            }
        }
        assert_eq!(Route::from_path("/v1/unknown"), None);
        assert_eq!(
            Route::Logs.rejected_record_field(),
            Some("rejectedLogRecords")
        );
        assert_eq!(Route::Traces.rejected_record_field(), Some("rejectedSpans"));
        assert_eq!(Route::Activity.rejected_record_field(), None);
    }
}
