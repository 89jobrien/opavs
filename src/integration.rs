//! Neutral integration targets, artifact descriptions, and catalog port.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// A supported agent-client integration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl Target {
    pub(crate) const ALL: [Target; 4] = [
        Target::Claude,
        Target::Codex,
        Target::Gemini,
        Target::Opencode,
    ];

    /// Return the stable lowercase CLI name for this integration target.
    pub const fn as_str(self) -> &'static str {
        match self {
            Target::Claude => "claude",
            Target::Codex => "codex",
            Target::Gemini => "gemini",
            Target::Opencode => "opencode",
        }
    }
}

/// Immutable artifact description shared by installation and diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactExpectation {
    pub(crate) path: PathBuf,
    pub(crate) owned: bool,
    pub(crate) expected: ArtifactMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactMatch {
    Exact(String),
    CodexHook,
    GeminiEnablement { home_glob: String },
    OpencodePlugin { plugin_ref: String },
}

impl ArtifactExpectation {
    /// Describe a target-owned artifact whose complete text is controlled by OPAVS.
    pub fn exact(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owned: true,
            expected: ArtifactMatch::Exact(content.into()),
        }
    }

    /// Describe shared Codex hooks containing the OPAVS pre-tool guard.
    pub fn codex_hook(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            owned: false,
            expected: ArtifactMatch::CodexHook,
        }
    }

    /// Describe shared Gemini enablement for the supplied home glob.
    pub fn gemini_enablement(path: impl Into<PathBuf>, home_glob: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owned: false,
            expected: ArtifactMatch::GeminiEnablement {
                home_glob: home_glob.into(),
            },
        }
    }

    /// Describe shared OpenCode configuration containing the supplied plugin reference.
    pub fn opencode_plugin(path: impl Into<PathBuf>, plugin_ref: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owned: false,
            expected: ArtifactMatch::OpencodePlugin {
                plugin_ref: plugin_ref.into(),
            },
        }
    }

    pub(crate) fn is_current(&self, contents: &str) -> bool {
        match &self.expected {
            ArtifactMatch::Exact(expected) => contents == expected,
            ArtifactMatch::CodexHook => {
                serde_json::from_str(contents).is_ok_and(|root| has_codex_pretool_hook(&root))
            }
            ArtifactMatch::GeminiEnablement { home_glob } => serde_json::from_str(contents)
                .is_ok_and(|root| has_gemini_enablement(&root, home_glob)),
            ArtifactMatch::OpencodePlugin { plugin_ref } => serde_json::from_str(contents)
                .is_ok_and(|root| has_opencode_plugin_entry(&root, plugin_ref)),
        }
    }

    pub(crate) fn indicates_install(&self, contents: &str) -> bool {
        self.owned
            || self.is_current(contents)
            || match &self.expected {
                ArtifactMatch::CodexHook => contents.contains("opavs guard"),
                ArtifactMatch::GeminiEnablement { .. } => contents.contains("\"opavs\""),
                ArtifactMatch::OpencodePlugin { .. } => contents.contains("opavs@file://"),
                ArtifactMatch::Exact(_) => false,
            }
    }
}

/// Supplies immutable integration artifact expectations to application services.
pub trait IntegrationCatalog {
    /// Return expected artifacts for one target and home directory.
    fn artifacts(&self, target: Target, home: &Path) -> Vec<ArtifactExpectation>;
}

pub(crate) fn is_opavs_guard_command(command: &str) -> bool {
    command.trim() == "opavs guard"
}

pub(crate) fn has_codex_pretool_hook(root: &Value) -> bool {
    root.pointer("/hooks/PreToolUse")
        .and_then(Value::as_array)
        .is_some_and(|pretool| {
            pretool.iter().any(|entry| {
                entry.get("matcher").and_then(Value::as_str) == Some("Edit|Write|Bash")
                    && entry
                        .get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|hooks| {
                            hooks.iter().any(|hook| {
                                hook.get("command")
                                    .and_then(Value::as_str)
                                    .is_some_and(is_opavs_guard_command)
                            })
                        })
            })
        })
}

fn has_gemini_enablement(root: &Value, home_glob: &str) -> bool {
    root.pointer("/opavs/overrides")
        .and_then(Value::as_array)
        .is_some_and(|overrides| {
            overrides
                .iter()
                .any(|override_path| override_path.as_str() == Some(home_glob))
        })
}

fn has_opencode_plugin_entry(root: &Value, plugin_ref: &str) -> bool {
    root.get("plugin")
        .and_then(Value::as_array)
        .is_some_and(|plugins| {
            plugins
                .iter()
                .any(|plugin| plugin.as_str() == Some(plugin_ref))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opavs_guard_command_accepts_surrounding_whitespace() {
        assert!(is_opavs_guard_command("  opavs guard\n"));
        assert!(!is_opavs_guard_command("echo opavs guard"));
    }
}
