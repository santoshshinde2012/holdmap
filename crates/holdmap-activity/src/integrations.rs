//! Reviewable configuration examples for supported agent exporters.
//!
//! Examples are returned to the caller only; this module never edits agent settings,
//! launches an agent, reads credentials from disk or enables telemetry automatically.

use crate::{client::LocalClient, CollectorCredentials, Provider};

/// A fixed, non-sensitive setup error.
#[derive(Debug, Clone, Copy)]
pub struct SetupError;

impl std::fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Choose Codex or Claude and use the active local recording session.")
    }
}

impl std::error::Error for SetupError {}

/// Produce a configuration example for one active local recording session.
///
/// Codex receives a TOML log-export example. Claude receives a trace-only shell example
/// (PowerShell on Windows); its enhanced tracing is currently a provider beta feature.
/// One signal is selected per example to avoid counting a tool result and span twice.
pub fn setup(
    provider: Provider,
    credentials: &CollectorCredentials,
    windows: bool,
) -> Result<String, SetupError> {
    LocalClient::from_credentials(credentials).map_err(|_| SetupError)?;
    match provider {
        Provider::Codex => Ok(format!(
            "# Merge into your user-level Codex config.toml, then restart Codex.\n\
             # Before launching Codex, set the export batch and clear compression overrides:\n\
             # {}\n\
             # Keep recording open and reconfigure for every new recording session.\n\
             # Review existing telemetry settings before replacing an exporter.\n\
             [otel]\n\
             log_user_prompt = false\n\
             exporter = {{ otlp-http = {{ endpoint = \"{}/v1/logs\", protocol = \"json\", headers = {{ Authorization = \"Bearer {}\" }} }} }}\n",
            if windows {
                "$env:OTEL_BLRP_MAX_EXPORT_BATCH_SIZE = '32'\n# Remove-Item Env:OTEL_EXPORTER_OTLP_COMPRESSION, Env:OTEL_EXPORTER_OTLP_LOGS_COMPRESSION -ErrorAction SilentlyContinue"
            } else {
                "export OTEL_BLRP_MAX_EXPORT_BATCH_SIZE=32\n# unset OTEL_EXPORTER_OTLP_COMPRESSION OTEL_EXPORTER_OTLP_LOGS_COMPRESSION"
            },
            credentials.endpoint, credentials.token
        )),
        Provider::Claude => {
            let variables = [
                ("CLAUDE_CODE_ENABLE_TELEMETRY", "1".to_owned()),
                ("CLAUDE_CODE_ENHANCED_TELEMETRY_BETA", "1".to_owned()),
                ("ENABLE_BETA_TRACING_DETAILED", "0".to_owned()),
                ("OTEL_TRACES_EXPORTER", "otlp".to_owned()),
                ("OTEL_LOGS_EXPORTER", "none".to_owned()),
                ("OTEL_METRICS_EXPORTER", "none".to_owned()),
                ("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "http/json".to_owned()),
                ("OTEL_BSP_MAX_EXPORT_BATCH_SIZE", "32".to_owned()),
                (
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                    format!("{}/v1/traces", credentials.endpoint),
                ),
                (
                    "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
                    format!("Authorization=Bearer {}", credentials.token),
                ),
                ("OTEL_LOG_USER_PROMPTS", "0".to_owned()),
                ("OTEL_LOG_ASSISTANT_RESPONSES", "0".to_owned()),
                ("OTEL_LOG_TOOL_DETAILS", "0".to_owned()),
                ("OTEL_LOG_TOOL_CONTENT", "0".to_owned()),
            ];
            let mut output = String::from(
                "# Claude enhanced tracing is beta; spans arrive after they finish.\n\
                 # Run in a new terminal while Holdmap recording stays open.\n\
                 # Review existing telemetry settings before replacing exporters.\n",
            );
            output.push_str("# Remove enabled OTEL_LOG_RAW_API_BODIES from user/managed settings too.\n# If managed policy requires content export or another destination, do not use this recipe.\n");
            for (name, value) in variables {
                if windows {
                    output.push_str(&format!("$env:{name} = '{value}'\n"));
                } else {
                    output.push_str(&format!("export {name}='{value}'\n"));
                }
            }
            output.push_str(if windows {
                "Remove-Item Env:OTEL_LOG_RAW_API_BODIES, Env:OTEL_EXPORTER_OTLP_COMPRESSION, Env:OTEL_EXPORTER_OTLP_TRACES_COMPRESSION -ErrorAction SilentlyContinue\n"
            } else {
                "unset OTEL_LOG_RAW_API_BODIES OTEL_EXPORTER_OTLP_COMPRESSION OTEL_EXPORTER_OTLP_TRACES_COMPRESSION\n"
            });
            output.push_str("claude\n");
            Ok(output)
        }
        _ => Err(SetupError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> CollectorCredentials {
        CollectorCredentials {
            endpoint: "http://127.0.0.1:12345".into(),
            token: "a".repeat(64),
        }
    }

    #[test]
    fn codex_example_uses_json_and_redacts_prompts() {
        let example = setup(Provider::Codex, &credentials(), false).unwrap();
        let parsed: toml::Value = toml::from_str(&example).unwrap();
        assert_eq!(parsed["otel"]["log_user_prompt"].as_bool(), Some(false));
        let exporter = &parsed["otel"]["exporter"]["otlp-http"];
        assert_eq!(exporter["protocol"].as_str(), Some("json"));
        assert_eq!(
            exporter["endpoint"].as_str(),
            Some("http://127.0.0.1:12345/v1/logs")
        );
        assert!(parsed["otel"].get("trace_exporter").is_none());
        assert!(example.contains("OTEL_BLRP_MAX_EXPORT_BATCH_SIZE=32"));
        assert!(example
            .contains("unset OTEL_EXPORTER_OTLP_COMPRESSION OTEL_EXPORTER_OTLP_LOGS_COMPRESSION"));
        assert!(!example.contains("COMPRESSION=none"));
        assert!(setup(Provider::Codex, &credentials(), true)
            .unwrap()
            .contains("$env:OTEL_BLRP_MAX_EXPORT_BATCH_SIZE = '32'"));
    }

    #[test]
    fn claude_examples_disable_content_and_use_one_signal() {
        for windows in [false, true] {
            let example = setup(Provider::Claude, &credentials(), windows).unwrap();
            assert!(example.contains("http/json"));
            assert!(example.contains("/v1/traces"));
            assert!(example.contains("ENHANCED_TELEMETRY_BETA"));
            for flag in [
                "OTEL_LOG_USER_PROMPTS",
                "OTEL_LOG_ASSISTANT_RESPONSES",
                "OTEL_LOG_TOOL_DETAILS",
                "OTEL_LOG_TOOL_CONTENT",
            ] {
                assert!(
                    example.contains(&format!("{flag}{}'0'", if windows { " = " } else { "=" }))
                );
            }
            assert!(example.contains(if windows {
                "Remove-Item Env:OTEL_LOG_RAW_API_BODIES"
            } else {
                "unset OTEL_LOG_RAW_API_BODIES"
            }));
            assert!(example.ends_with("claude\n"));
        }
    }

    #[test]
    fn setup_cannot_inject_shell_toml_or_nonlocal_endpoint() {
        let mut invalid = credentials();
        invalid.token = "a'$(private)".repeat(8);
        assert!(setup(Provider::Claude, &invalid, false).is_err());
        invalid = credentials();
        invalid.endpoint = "http://example.invalid:4318".into();
        assert!(setup(Provider::Codex, &invalid, false).is_err());
        assert!(setup(Provider::OpenTelemetry, &credentials(), false).is_err());
    }
}
