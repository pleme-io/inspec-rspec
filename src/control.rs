use serde::{Deserialize, Serialize};

/// A parsed InSpec control.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InSpecControl {
    /// Control identifier (e.g., `"sshd-01"`).
    pub id: String,

    /// Impact score from 0.0 (informational) to 1.0 (critical).
    pub impact: f64,

    /// Human-readable title.
    pub title: String,

    /// Longer description of the control.
    pub description: String,

    /// Tags attached to the control (NIST, CIS, etc.).
    pub tags: Vec<InSpecTag>,

    /// Test blocks (`describe` blocks) within the control.
    pub tests: Vec<InSpecTest>,

    /// Source filename this control was parsed from.
    pub source_file: String,

    /// Line number where the control block starts.
    pub source_line: usize,
}

/// A tag on an InSpec control (e.g., `tag nist: ['AC-7']`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InSpecTag {
    /// Tag key (e.g., `"nist"`, `"cis"`, `"severity"`).
    pub key: String,

    /// Tag values. Single-value tags have one element.
    pub values: Vec<String>,
}

/// A single `describe` block within an InSpec control.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InSpecTest {
    /// The InSpec resource type (e.g., `"sshd_config"`, `"file"`, `"command"`).
    pub resource_type: String,

    /// Arguments passed to the resource constructor (may be empty).
    pub resource_args: String,

    /// Matcher assertions within the describe block.
    pub matchers: Vec<InSpecMatcher>,
}

/// A matcher assertion (e.g., `its('Protocol') { should cmp 2 }`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InSpecMatcher {
    /// Property name from `its('property')`, or `None` for bare `it { should ... }`.
    pub property: Option<String>,

    /// The expectation string (e.g., `"cmp 2"`, `"eq 'yes'"`, `"be_installed"`).
    pub expectation: String,

    /// Whether this is a negated assertion (`should_not`).
    pub negated: bool,

    /// The raw source line for reference.
    pub raw_line: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_control() {
        let control = InSpecControl {
            id: "test-01".to_string(),
            impact: 0.7,
            title: "Test Control".to_string(),
            description: "A test control".to_string(),
            tags: vec![InSpecTag {
                key: "nist".to_string(),
                values: vec!["AC-7".to_string()],
            }],
            tests: vec![InSpecTest {
                resource_type: "sshd_config".to_string(),
                resource_args: String::new(),
                matchers: vec![InSpecMatcher {
                    property: Some("Protocol".to_string()),
                    expectation: "cmp 2".to_string(),
                    negated: false,
                    raw_line: "its('Protocol') { should cmp 2 }".to_string(),
                }],
            }],
            source_file: "test.rb".to_string(),
            source_line: 1,
        };

        let json = serde_json::to_string(&control).unwrap();
        let deserialized: InSpecControl = serde_json::from_str(&json).unwrap();
        assert_eq!(control, deserialized);
    }

    #[test]
    fn serde_roundtrip_tag() {
        let tag = InSpecTag {
            key: "cis".to_string(),
            values: vec!["5.2.1".to_string()],
        };

        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: InSpecTag = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, deserialized);
    }

    #[test]
    fn serde_roundtrip_tag_multiple_values() {
        let tag = InSpecTag {
            key: "nist".to_string(),
            values: vec!["AC-17(2)".to_string(), "SC-8".to_string()],
        };

        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: InSpecTag = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, deserialized);
    }

    #[test]
    fn serde_roundtrip_matcher_with_property() {
        let matcher = InSpecMatcher {
            property: Some("MaxAuthTries".to_string()),
            expectation: "cmp <= 4".to_string(),
            negated: false,
            raw_line: "its('MaxAuthTries') { should cmp <= 4 }".to_string(),
        };

        let json = serde_json::to_string(&matcher).unwrap();
        let deserialized: InSpecMatcher = serde_json::from_str(&json).unwrap();
        assert_eq!(matcher, deserialized);
    }

    #[test]
    fn serde_roundtrip_matcher_negated() {
        let matcher = InSpecMatcher {
            property: None,
            expectation: "be_installed".to_string(),
            negated: true,
            raw_line: "it { should_not be_installed }".to_string(),
        };

        let json = serde_json::to_string(&matcher).unwrap();
        let deserialized: InSpecMatcher = serde_json::from_str(&json).unwrap();
        assert_eq!(matcher, deserialized);
    }

    #[test]
    fn serde_roundtrip_test() {
        let test = InSpecTest {
            resource_type: "command".to_string(),
            resource_args: "'uname -r'".to_string(),
            matchers: vec![
                InSpecMatcher {
                    property: Some("stdout".to_string()),
                    expectation: "match /5\\.4/".to_string(),
                    negated: false,
                    raw_line: "its('stdout') { should match /5\\.4/ }".to_string(),
                },
                InSpecMatcher {
                    property: Some("exit_status".to_string()),
                    expectation: "eq 0".to_string(),
                    negated: false,
                    raw_line: "its('exit_status') { should eq 0 }".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&test).unwrap();
        let deserialized: InSpecTest = serde_json::from_str(&json).unwrap();
        assert_eq!(test, deserialized);
    }

    #[test]
    fn control_default_values() {
        let control = InSpecControl {
            id: "empty-01".to_string(),
            impact: 0.0,
            title: String::new(),
            description: String::new(),
            tags: vec![],
            tests: vec![],
            source_file: "empty.rb".to_string(),
            source_line: 0,
        };

        assert!(control.tags.is_empty());
        assert!(control.tests.is_empty());
        assert_eq!(control.impact, 0.0);
    }
}
