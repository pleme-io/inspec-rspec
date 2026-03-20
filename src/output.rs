use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::helpers;
use crate::transpiler::RSpecFile;

/// Write generated RSpec files to the output directory.
///
/// Creates `spec/compliance/` subdirectory and writes each file.
///
/// # Errors
///
/// Returns `Error::Io` if file or directory operations fail.
pub fn write_rspec_files(files: &[RSpecFile], output_dir: &Path) -> Result<Vec<String>> {
    let compliance_dir = output_dir.join("compliance");
    fs::create_dir_all(&compliance_dir).map_err(|e| Error::Io {
        path: compliance_dir.clone(),
        source: e,
    })?;

    let mut written = Vec::new();

    for file in files {
        let path = compliance_dir.join(&file.filename);
        fs::write(&path, &file.content).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        written.push(path.display().to_string());
    }

    Ok(written)
}

/// Write the `compliance_helpers.rb` support file.
///
/// # Errors
///
/// Returns `Error::Io` if the file cannot be written.
pub fn write_helpers(output_dir: &Path) -> Result<String> {
    fs::create_dir_all(output_dir).map_err(|e| Error::Io {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let path = output_dir.join("compliance_helpers.rb");
    let content = helpers::generate_helpers();
    fs::write(&path, &content).map_err(|e| Error::Io {
        path: path.clone(),
        source: e,
    })?;

    Ok(path.display().to_string())
}

/// Write the `spec_helper.rb` file for the profile.
///
/// # Errors
///
/// Returns `Error::Io` if the file cannot be written.
pub fn write_spec_helper(output_dir: &Path, profile_name: &str) -> Result<String> {
    fs::create_dir_all(output_dir).map_err(|e| Error::Io {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let path = output_dir.join("spec_helper.rb");
    let content = helpers::generate_spec_helper(profile_name);
    fs::write(&path, &content).map_err(|e| Error::Io {
        path: path.clone(),
        source: e,
    })?;

    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_rspec_files_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("spec");

        let files = vec![RSpecFile {
            filename: "test_01_spec.rb".to_string(),
            content: "# test".to_string(),
            control_id: "test-01".to_string(),
        }];

        let result = write_rspec_files(&files, &output_dir).unwrap();
        assert_eq!(result.len(), 1);
        assert!(output_dir.join("compliance").exists());
    }

    #[test]
    fn write_rspec_files_content_matches() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("spec");
        let expected_content = "# generated test content\nRSpec.describe 'test' do\nend\n";

        let files = vec![RSpecFile {
            filename: "test_01_spec.rb".to_string(),
            content: expected_content.to_string(),
            control_id: "test-01".to_string(),
        }];

        write_rspec_files(&files, &output_dir).unwrap();

        let written = fs::read_to_string(output_dir.join("compliance/test_01_spec.rb")).unwrap();
        assert_eq!(written, expected_content);
    }

    #[test]
    fn write_rspec_files_multiple() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("spec");

        let files = vec![
            RSpecFile {
                filename: "a_spec.rb".to_string(),
                content: "# a".to_string(),
                control_id: "a".to_string(),
            },
            RSpecFile {
                filename: "b_spec.rb".to_string(),
                content: "# b".to_string(),
                control_id: "b".to_string(),
            },
        ];

        let result = write_rspec_files(&files, &output_dir).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn write_helpers_creates_file() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("spec");

        let path = write_helpers(&output_dir).unwrap();
        assert!(Path::new(&path).exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("module ComplianceHelpers"));
    }

    #[test]
    fn write_spec_helper_creates_file() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("spec");

        let path = write_spec_helper(&output_dir, "test-profile").unwrap();
        assert!(Path::new(&path).exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-profile"));
    }

    #[test]
    fn write_helpers_deterministic() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();

        write_helpers(tmp1.path()).unwrap();
        write_helpers(tmp2.path()).unwrap();

        let c1 = fs::read_to_string(tmp1.path().join("compliance_helpers.rb")).unwrap();
        let c2 = fs::read_to_string(tmp2.path().join("compliance_helpers.rb")).unwrap();
        assert_eq!(c1, c2);
    }
}
