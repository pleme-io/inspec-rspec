use inspec_rspec::output;
use inspec_rspec::parser;
use inspec_rspec::transpiler;

use tempfile::TempDir;

/// Realistic SSH baseline InSpec control source.
const SSH_BASELINE: &str = r#"
control 'sshd-01' do
  impact 1.0
  title 'SSH Protocol Version'
  desc 'Ensure SSH Protocol is set to 2'
  tag nist: ['AC-17(2)', 'SC-8']
  tag cis: '5.2.1'

  describe sshd_config do
    its('Protocol') { should cmp 2 }
  end
end

control 'sshd-02' do
  impact 0.7
  title 'SSH MaxAuthTries'
  desc 'Limit authentication attempts'
  tag nist: ['AC-7']

  describe sshd_config do
    its('MaxAuthTries') { should cmp <= 4 }
  end
end

control 'sshd-03' do
  impact 1.0
  title 'SSH Root Login'
  desc 'Disable root login via SSH'
  tag nist: ['AC-6(2)']
  tag cis: '5.2.8'

  describe sshd_config do
    its('PermitRootLogin') { should eq 'no' }
  end
end
"#;

/// CIS-style control with multiple describe blocks.
const CIS_MULTI_DESCRIBE: &str = r#"
control 'cis-1.1.1' do
  impact 1.0
  title 'Ensure mounting of cramfs filesystems is disabled'
  desc 'The cramfs filesystem type is a compressed read-only Linux filesystem'
  tag nist: ['CM-7']
  tag cis: '1.1.1.1'
  tag severity: 'high'

  describe command('modprobe -n -v cramfs') do
    its('stdout') { should match /install \/bin\/true/ }
  end

  describe command('lsmod') do
    its('stdout') { should_not match /cramfs/ }
  end

  describe service('cramfs') do
    it { should_not be_running }
  end
end
"#;

#[test]
fn integration_parse_ssh_baseline() {
    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();

    assert_eq!(controls.len(), 3);
    assert_eq!(controls[0].id, "sshd-01");
    assert_eq!(controls[1].id, "sshd-02");
    assert_eq!(controls[2].id, "sshd-03");
}

#[test]
fn integration_ssh_baseline_transpile() {
    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "ssh-baseline");

    assert_eq!(files.len(), 3);

    // All files should be valid Ruby-ish (contain RSpec.describe and end)
    for file in &files {
        assert!(file.content.contains("RSpec.describe"), "Missing RSpec.describe in {}", file.filename);
        assert!(file.content.contains("end\n"), "Missing end in {}", file.filename);
        assert!(file.content.contains("require 'compliance_helpers'"), "Missing require in {}", file.filename);
    }
}

#[test]
fn integration_ssh_baseline_nist_tags() {
    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "ssh-baseline");

    // sshd-01 should have AC-17(2) and SC-8
    assert!(files[0].content.contains("AC-17(2)"));
    assert!(files[0].content.contains("SC-8"));

    // sshd-02 should have AC-7
    assert!(files[1].content.contains("AC-7"));
}

#[test]
fn integration_ssh_baseline_write_output() {
    let tmp = TempDir::new().unwrap();
    let output_dir = tmp.path().join("spec");

    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "ssh-baseline");

    let written = output::write_rspec_files(&files, &output_dir).unwrap();
    assert_eq!(written.len(), 3);

    // All files should exist on disk
    for path in &written {
        assert!(
            std::path::Path::new(path).exists(),
            "File should exist: {path}"
        );
    }
}

#[test]
fn integration_ssh_baseline_deterministic() {
    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();

    let files1 = transpiler::transpile_profile(&controls, "ssh-baseline");
    let files2 = transpiler::transpile_profile(&controls, "ssh-baseline");

    assert_eq!(files1.len(), files2.len());
    for (f1, f2) in files1.iter().zip(files2.iter()) {
        assert_eq!(f1.content, f2.content, "Determinism violated for {}", f1.filename);
        assert_eq!(f1.filename, f2.filename);
    }
}

#[test]
fn integration_cis_multi_describe_parse() {
    let controls = parser::parse_controls(CIS_MULTI_DESCRIBE, "cis.rb").unwrap();

    assert_eq!(controls.len(), 1);
    let control = &controls[0];

    assert_eq!(control.id, "cis-1.1.1");
    assert_eq!(control.tests.len(), 3);
    assert_eq!(control.tags.len(), 3);
}

#[test]
fn integration_cis_multi_describe_transpile() {
    let controls = parser::parse_controls(CIS_MULTI_DESCRIBE, "cis.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "cis-baseline");

    assert_eq!(files.len(), 1);
    let content = &files[0].content;

    // All three describe block resources should be present
    assert!(content.contains("# InSpec resource: command"));
    assert!(content.contains("# InSpec resource: service"));

    // Negated matcher should produce to_not
    assert!(content.contains("to_not"));
}

#[test]
fn integration_cis_all_tests_preserved() {
    let controls = parser::parse_controls(CIS_MULTI_DESCRIBE, "cis.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "cis-baseline");

    let content = &files[0].content;

    // Count "it '" blocks — should be 3 (one per matcher)
    let it_count = content.matches("it '").count();
    assert_eq!(it_count, 3, "Expected 3 it blocks, got {it_count}");
}

#[test]
fn integration_cis_severity_tag() {
    let controls = parser::parse_controls(CIS_MULTI_DESCRIBE, "cis.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "cis-baseline");

    assert!(files[0].content.contains("metadata[:severity] = 'high'"));
}

#[test]
fn integration_full_pipeline_with_helpers() {
    let tmp = TempDir::new().unwrap();
    let output_dir = tmp.path().join("spec");

    let controls = parser::parse_controls(SSH_BASELINE, "ssh_config.rb").unwrap();
    let files = transpiler::transpile_profile(&controls, "ssh-baseline");

    output::write_rspec_files(&files, &output_dir).unwrap();
    output::write_helpers(&output_dir).unwrap();
    output::write_spec_helper(&output_dir, "ssh-baseline").unwrap();

    // Verify all expected files exist
    assert!(output_dir.join("compliance_helpers.rb").exists());
    assert!(output_dir.join("spec_helper.rb").exists());
    assert!(output_dir.join("compliance").exists());

    // Verify compliance dir has the right number of files
    let count = std::fs::read_dir(output_dir.join("compliance"))
        .unwrap()
        .count();
    assert_eq!(count, 3);
}
