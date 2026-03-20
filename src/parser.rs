use regex::Regex;

use crate::control::{InSpecControl, InSpecMatcher, InSpecTag, InSpecTest};
use crate::error::{Error, Result};

/// Parse an InSpec control file into a list of controls.
///
/// Uses regex-based parsing rather than a full Ruby AST parser.
/// InSpec controls follow a strict enough DSL that regex works reliably.
///
/// # Errors
///
/// Returns `Error::Parse` if the control file contains malformed blocks.
pub fn parse_controls(source: &str, filename: &str) -> Result<Vec<InSpecControl>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut controls = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match: control 'id' do  or  control "id" do
        if let Some(id) = extract_control_id(trimmed) {
            let start_line = i + 1; // 1-indexed
            let block_end = find_block_end(&lines, i)?;
            let block_lines = &lines[i + 1..block_end];

            let control = parse_control_block(&id, block_lines, filename, start_line)?;
            controls.push(control);

            i = block_end + 1;
        } else {
            i += 1;
        }
    }

    Ok(controls)
}

/// Extract control ID from a `control 'id' do` line.
fn extract_control_id(line: &str) -> Option<String> {
    let re = Regex::new(r#"^\s*control\s+['"]([^'"]+)['"]\s+do\s*$"#).ok()?;
    re.captures(line).map(|c| c[1].to_string())
}

/// Find the matching `end` for a `do` block starting at `start_idx`.
fn find_block_end(lines: &[&str], start_idx: usize) -> Result<usize> {
    let mut depth: i32 = 1;

    for (offset, line) in lines.iter().enumerate().skip(start_idx + 1) {
        let trimmed = line.trim();

        // Skip comments and empty lines for nesting
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        // Count do/end pairs
        if is_block_opener(trimmed) {
            depth += 1;
        }
        if trimmed == "end" || trimmed.starts_with("end ") || trimmed.ends_with("end") && trimmed.len() > 3 && trimmed.as_bytes()[trimmed.len() - 4] == b' ' {
            // Only count standalone "end"
            if trimmed == "end" {
                depth -= 1;
                if depth == 0 {
                    return Ok(offset);
                }
            }
        }
    }

    Err(Error::Parse {
        file: String::new(),
        line: start_idx + 1,
        message: "unclosed control block — no matching 'end' found".to_string(),
    })
}

/// Check if a line opens a new block (contains `do` at end or `do |...|`).
fn is_block_opener(line: &str) -> bool {
    let trimmed = line.trim();
    // Matches lines ending with "do" or "do |...|"
    if trimmed.ends_with(" do") || trimmed == "do" {
        return true;
    }
    // Match "do |var|" style
    let re = Regex::new(r"\bdo\s*\|[^|]*\|\s*$").unwrap();
    re.is_match(trimmed)
}

/// Parse the body of a control block into an `InSpecControl`.
fn parse_control_block(
    id: &str,
    lines: &[&str],
    filename: &str,
    start_line: usize,
) -> Result<InSpecControl> {
    let mut impact = 0.5; // default
    let mut title = String::new();
    let mut description = String::new();
    let mut tags = Vec::new();
    let mut tests = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip comments and empty lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            i += 1;
            continue;
        }

        // impact X.Y
        if let Some(imp) = parse_impact(trimmed) {
            impact = imp;
            i += 1;
            continue;
        }

        // title 'text'
        if let Some(t) = parse_string_field(trimmed, "title") {
            title = t;
            i += 1;
            continue;
        }

        // desc 'text' (single line)
        if let Some(d) = parse_string_field(trimmed, "desc") {
            description = d;
            i += 1;
            continue;
        }

        // tag key: value
        if let Some(tag) = parse_tag(trimmed) {
            tags.push(tag);
            i += 1;
            continue;
        }

        // describe resource(args) do ... end
        if trimmed.starts_with("describe ") && (trimmed.ends_with(" do") || trimmed.ends_with(" do")) {
            let block_end = find_describe_end(lines, i);
            let describe_lines = &lines[i..=block_end];
            if let Some(test) = parse_describe_block(describe_lines) {
                tests.push(test);
            }
            i = block_end + 1;
            continue;
        }

        i += 1;
    }

    Ok(InSpecControl {
        id: id.to_string(),
        impact,
        title,
        description,
        tags,
        tests,
        source_file: filename.to_string(),
        source_line: start_line,
    })
}

/// Parse `impact X.Y` from a line.
fn parse_impact(line: &str) -> Option<f64> {
    let re = Regex::new(r"^\s*impact\s+([\d.]+)\s*$").ok()?;
    re.captures(line)
        .and_then(|c| c[1].parse::<f64>().ok())
}

/// Parse a single-quoted or double-quoted string field (e.g., `title 'text'`).
fn parse_string_field(line: &str, field: &str) -> Option<String> {
    let pattern = format!(r#"^\s*{field}\s+['"](.+?)['"]\s*$"#);
    let re = Regex::new(&pattern).ok()?;
    re.captures(line).map(|c| c[1].to_string())
}

/// Parse a tag line like `tag nist: ['AC-7', 'SC-8']` or `tag cis: '5.2.1'`.
fn parse_tag(line: &str) -> Option<InSpecTag> {
    let re = Regex::new(r"^\s*tag\s+(\w+):\s*(.+?)\s*$").ok()?;
    let caps = re.captures(line)?;
    let key = caps[1].to_string();
    let raw_value = &caps[2];

    let values = parse_tag_values(raw_value);
    Some(InSpecTag { key, values })
}

/// Parse tag values from raw string — handles arrays and single values.
fn parse_tag_values(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();

    // Array form: ['val1', 'val2']
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let re = Regex::new(r#"['"]([^'"]+)['"]"#).unwrap();
        return re
            .captures_iter(inner)
            .map(|c| c[1].to_string())
            .collect();
    }

    // Single quoted value: 'val'
    let re = Regex::new(r#"^['"](.+?)['"]$"#).unwrap();
    if let Some(caps) = re.captures(trimmed) {
        return vec![caps[1].to_string()];
    }

    // Bare value
    vec![trimmed.to_string()]
}

/// Find the end of a `describe` block starting at `start_idx`.
fn find_describe_end(lines: &[&str], start_idx: usize) -> usize {
    let mut depth: i32 = 0;

    for (offset, line) in lines.iter().enumerate().skip(start_idx) {
        let trimmed = line.trim();

        if is_block_opener(trimmed) {
            depth += 1;
        }
        if trimmed == "end" {
            depth -= 1;
            if depth == 0 {
                return offset;
            }
        }
    }

    // If no matching end found, return last line
    lines.len() - 1
}

/// Parse a `describe resource(args) do ... end` block.
fn parse_describe_block(lines: &[&str]) -> Option<InSpecTest> {
    let first_line = lines.first()?.trim();

    let (resource_type, resource_args) = parse_describe_header(first_line)?;

    let mut matchers = Vec::new();

    for line in &lines[1..] {
        let trimmed = line.trim();

        // its('property') { should matcher }
        if let Some(m) = parse_its_matcher(trimmed) {
            matchers.push(m);
            continue;
        }

        // it { should matcher }
        if let Some(m) = parse_it_matcher(trimmed) {
            matchers.push(m);
        }
    }

    Some(InSpecTest {
        resource_type,
        resource_args,
        matchers,
    })
}

/// Parse `describe resource(args) do` header.
fn parse_describe_header(line: &str) -> Option<(String, String)> {
    // describe resource_name do
    let re_no_args = Regex::new(r"^\s*describe\s+(\w+)\s+do\s*$").ok()?;
    if let Some(caps) = re_no_args.captures(line) {
        return Some((caps[1].to_string(), String::new()));
    }

    // describe resource_name(args) do
    let re_with_args = Regex::new(r"^\s*describe\s+(\w+)\((.+?)\)\s+do\s*$").ok()?;
    if let Some(caps) = re_with_args.captures(line) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    // describe resource_name('arg') do
    let re_quoted = Regex::new(r#"^\s*describe\s+(\w+)\s*\(\s*['"](.+?)['"]\s*\)\s+do\s*$"#).ok()?;
    if let Some(caps) = re_quoted.captures(line) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    None
}

/// Parse `its('property') { should matcher }` or `its('property') { should_not matcher }`.
fn parse_its_matcher(line: &str) -> Option<InSpecMatcher> {
    let re = Regex::new(
        r#"^\s*its\(\s*['"]([^'"]+)['"]\s*\)\s*\{\s*(should(?:_not)?)\s+(.+?)\s*\}\s*$"#,
    )
    .ok()?;

    let caps = re.captures(line)?;
    let property = caps[1].to_string();
    let should = &caps[2];
    let expectation = caps[3].to_string();
    let negated = should == "should_not";

    Some(InSpecMatcher {
        property: Some(property),
        expectation,
        negated,
        raw_line: line.trim().to_string(),
    })
}

/// Parse `it { should matcher }` or `it { should_not matcher }`.
fn parse_it_matcher(line: &str) -> Option<InSpecMatcher> {
    let re = Regex::new(r"^\s*it\s*\{\s*(should(?:_not)?)\s+(.+?)\s*\}\s*$").ok()?;

    let caps = re.captures(line)?;
    let should = &caps[1];
    let expectation = caps[2].to_string();
    let negated = should == "should_not";

    Some(InSpecMatcher {
        property: None,
        expectation,
        negated,
        raw_line: line.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_control() {
        let source = r#"
control 'test-01' do
  impact 1.0
  title 'Test Control'
  desc 'A test description'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].id, "test-01");
    }

    #[test]
    fn parse_multi_control_file() {
        let source = r#"
control 'ssh-01' do
  impact 1.0
  title 'First'
  desc 'First control'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end

control 'ssh-02' do
  impact 0.7
  title 'Second'
  desc 'Second control'
  describe sshd_config do
    its('MaxAuthTries') { should cmp 4 }
  end
end
"#;

        let controls = parse_controls(source, "ssh.rb").unwrap();
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].id, "ssh-01");
        assert_eq!(controls[1].id, "ssh-02");
    }

    #[test]
    fn parse_impact_extraction() {
        let source = r#"
control 'imp-01' do
  impact 0.3
  title 'Low Impact'
  desc 'Low impact control'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert!((controls[0].impact - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_title_extraction() {
        let source = r#"
control 'title-01' do
  title 'My Special Title'
  desc 'desc'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].title, "My Special Title");
    }

    #[test]
    fn parse_desc_extraction() {
        let source = r#"
control 'desc-01' do
  title 'Title'
  desc 'This is a detailed description of the control'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(
            controls[0].description,
            "This is a detailed description of the control"
        );
    }

    #[test]
    fn parse_tag_single_value() {
        let source = r#"
control 'tag-01' do
  title 'Tags'
  desc 'Test tags'
  tag cis: '5.2.1'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tags.len(), 1);
        assert_eq!(controls[0].tags[0].key, "cis");
        assert_eq!(controls[0].tags[0].values, vec!["5.2.1"]);
    }

    #[test]
    fn parse_tag_array_values() {
        let source = r#"
control 'tag-02' do
  title 'Tags'
  desc 'Test tags'
  tag nist: ['AC-17(2)', 'SC-8']
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tags.len(), 1);
        assert_eq!(controls[0].tags[0].key, "nist");
        assert_eq!(
            controls[0].tags[0].values,
            vec!["AC-17(2)", "SC-8"]
        );
    }

    #[test]
    fn parse_describe_block() {
        let source = r#"
control 'desc-block-01' do
  title 'Describe'
  desc 'Test describe'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tests.len(), 1);
        assert_eq!(controls[0].tests[0].resource_type, "sshd_config");
        assert!(controls[0].tests[0].resource_args.is_empty());
    }

    #[test]
    fn parse_its_matcher() {
        let source = r#"
control 'matcher-01' do
  title 'Matcher'
  desc 'Test matcher'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert_eq!(matcher.property, Some("Protocol".to_string()));
        assert_eq!(matcher.expectation, "cmp 2");
        assert!(!matcher.negated);
    }

    #[test]
    fn parse_should_not_matcher() {
        let source = r#"
control 'neg-01' do
  title 'Negated'
  desc 'Test negation'
  describe package('telnet') do
    it { should_not be_installed }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert!(matcher.property.is_none());
        assert_eq!(matcher.expectation, "be_installed");
        assert!(matcher.negated);
    }

    #[test]
    fn parse_cmp_operators() {
        let source = r#"
control 'cmp-01' do
  title 'Cmp Operators'
  desc 'Test comparison operators'
  describe sshd_config do
    its('MaxAuthTries') { should cmp <= 4 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert_eq!(matcher.expectation, "cmp <= 4");
    }

    #[test]
    fn parse_nested_describe() {
        let source = r#"
control 'nest-01' do
  title 'Nested'
  desc 'Test nested describe'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
    its('MaxAuthTries') { should cmp <= 4 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tests[0].matchers.len(), 2);
    }

    #[test]
    fn parse_multi_line_with_comments() {
        let source = r#"
# This is a comment
control 'comment-01' do
  # Another comment
  impact 1.0
  title 'With Comments'
  desc 'Control with comments'
  # describe block
  describe sshd_config do
    # check protocol
    its('Protocol') { should cmp 2 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].title, "With Comments");
    }

    #[test]
    fn parse_no_tests_control() {
        let source = r#"
control 'empty-01' do
  impact 0.5
  title 'No Tests'
  desc 'Control with no describe blocks'
  tag nist: ['AC-1']
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tests.len(), 0);
        assert_eq!(controls[0].tags.len(), 1);
    }

    #[test]
    fn parse_no_tags_control() {
        let source = r#"
control 'notag-01' do
  impact 0.5
  title 'No Tags'
  desc 'Control without tags'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert!(controls[0].tags.is_empty());
    }

    #[test]
    fn parse_empty_source() {
        let controls = parse_controls("", "empty.rb").unwrap();
        assert!(controls.is_empty());
    }

    #[test]
    fn parse_comments_only() {
        let source = r#"
# Just comments
# No controls here
"#;

        let controls = parse_controls(source, "comments.rb").unwrap();
        assert!(controls.is_empty());
    }

    #[test]
    fn parse_source_file_preserved() {
        let source = r#"
control 'src-01' do
  title 'Source'
  desc 'Test source tracking'
end
"#;

        let controls = parse_controls(source, "my_controls.rb").unwrap();
        assert_eq!(controls[0].source_file, "my_controls.rb");
    }

    #[test]
    fn parse_multiple_tags() {
        let source = r#"
control 'multitag-01' do
  title 'Multi Tags'
  desc 'Multiple tags'
  tag nist: ['AC-7']
  tag cis: '5.2.1'
  tag severity: 'high'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tags.len(), 3);
    }

    #[test]
    fn parse_multiple_describe_blocks() {
        let source = r#"
control 'multi-desc-01' do
  title 'Multiple Describes'
  desc 'Multiple describe blocks'
  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
  describe file('/etc/ssh/sshd_config') do
    it { should be_owned_by 'root' }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tests.len(), 2);
        assert_eq!(controls[0].tests[0].resource_type, "sshd_config");
        assert_eq!(controls[0].tests[1].resource_type, "file");
    }

    #[test]
    fn parse_it_matcher_bare() {
        let source = r#"
control 'it-01' do
  title 'Bare It'
  desc 'Bare it block'
  describe service('sshd') do
    it { should be_running }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert!(matcher.property.is_none());
        assert_eq!(matcher.expectation, "be_running");
        assert!(!matcher.negated);
    }

    #[test]
    fn parse_double_quoted_control_id() {
        let source = r#"
control "dq-01" do
  title 'Double Quoted ID'
  desc 'Uses double quotes for ID'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].id, "dq-01");
    }

    #[test]
    fn parse_resource_with_args() {
        let source = r#"
control 'args-01' do
  title 'Resource Args'
  desc 'Resource with arguments'
  describe package('openssh-server') do
    it { should be_installed }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert_eq!(controls[0].tests[0].resource_type, "package");
        // The args contain the quoted string
        assert!(controls[0].tests[0].resource_args.contains("openssh-server"));
    }

    #[test]
    fn parse_match_expectation() {
        let source = r#"
control 'match-01' do
  title 'Match'
  desc 'Test match'
  describe command('uname -r') do
    its('stdout') { should match /5\.4/ }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert_eq!(matcher.expectation, "match /5\\.4/");
    }

    #[test]
    fn parse_eq_string_expectation() {
        let source = r#"
control 'eq-01' do
  title 'Eq String'
  desc 'Test eq string'
  describe sshd_config do
    its('PermitRootLogin') { should eq 'no' }
  end
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        let matcher = &controls[0].tests[0].matchers[0];
        assert_eq!(matcher.expectation, "eq 'no'");
    }

    #[test]
    fn parse_impact_default() {
        let source = r#"
control 'default-impact' do
  title 'No Impact'
  desc 'Missing impact should default to 0.5'
end
"#;

        let controls = parse_controls(source, "test.rb").unwrap();
        assert!((controls[0].impact - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_control_id_single_quotes() {
        assert_eq!(
            extract_control_id("control 'sshd-01' do"),
            Some("sshd-01".to_string())
        );
    }

    #[test]
    fn extract_control_id_double_quotes() {
        assert_eq!(
            extract_control_id(r#"control "sshd-01" do"#),
            Some("sshd-01".to_string())
        );
    }

    #[test]
    fn extract_control_id_no_match() {
        assert_eq!(extract_control_id("# not a control"), None);
    }

    #[test]
    fn tag_values_array() {
        let values = parse_tag_values("['AC-7', 'SC-8']");
        assert_eq!(values, vec!["AC-7", "SC-8"]);
    }

    #[test]
    fn tag_values_single() {
        let values = parse_tag_values("'5.2.1'");
        assert_eq!(values, vec!["5.2.1"]);
    }

    #[test]
    fn tag_values_bare() {
        let values = parse_tag_values("high");
        assert_eq!(values, vec!["high"]);
    }
}
