# inspec-rspec -- Deterministic InSpec to RSpec compliance test transpiler

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


Converts InSpec compliance profiles into standalone RSpec tests that run without the InSpec CLI. Produces deterministic output suitable for cryptographic attestation in the tameshi ecosystem. The transpiled test results feed into tameshi's `InSpecResultCollector` and `RSpecResultCollector` as attestation layer inputs. Edition 2024, Rust 1.89.0, MIT.

## Build

```bash
cargo check
cargo test          # 94 tests (84 lib + 10 integration)
cargo build --release
```

## Test Breakdown

| Category | Count | What It Covers |
|----------|------:|----------------|
| Unit (lib) | 84 | Parser, control types, transpiler, output, helpers, error types |
| Integration (tests/) | 10 | Full pipeline: parse SSH baseline -> transpile -> write -> hash |
| **Total** | **94** | |

## Purpose in the Proof Chain

```
InSpec profiles (controls/*.rb)
        |
        v
inspec-rspec transpile
        |
        v
RSpec test files (spec/compliance/*_spec.rb)
        |
        v
RSpec JSON reporter output
        |
        +--> tameshi::collectors::rspec::hash_rspec_output()
        |       -> LayerSignature(RSpecResult)
        |
        +--> tameshi::collectors::inspec_result::hash_inspec_output()
                -> LayerSignature(InSpecResult)
                        |
                        v
              compose_certification_artifact()
                Leaf 1: control_hash
                        |
                        v
              CertificationArtifact.composed_root
                        |
                        v
              SignatureGate CRD -> BPF allow map -> kanshi enforcement
```

The transpiler's output is deterministic by construction: same InSpec input always produces identical RSpec output. This determinism is verified by 10 integration tests that compare hashes across multiple transpilation runs.

## Architecture

```
src/
  lib.rs              -- 6 module declarations (control, error, helpers, output, parser, transpiler)
  main.rs             -- CLI: transpile, inspect, hash subcommands
  control.rs          -- InSpecControl, InSpecTag, InSpecTest, InSpecMatcher types
  parser.rs           -- parse_controls() regex-based InSpec DSL parser
  transpiler.rs       -- transpile_profile() -> Vec<RSpecFile>
  output.rs           -- write_rspec_files(), write_helpers(), write_spec_helper()
  helpers.rs          -- generate_helpers() -> ComplianceHelpers Ruby module
  error.rs            -- Error enum (Parse, Transpile, Io, InvalidProfile, NoControls)
tests/
  transpile_test.rs   -- 10 integration tests
```

## CLI

```bash
# Transpile InSpec profile to RSpec tests
inspec-rspec transpile /path/to/profile --output generated/spec

# Parse and display controls as JSON (without generating output)
inspec-rspec inspect /path/to/profile

# Hash the generated output for attestation
inspec-rspec hash generated/spec
```

## All Types

### Core Types (control.rs)

| Type | Fields | Purpose |
|------|--------|---------|
| `InSpecControl` | `id: String`, `impact: f64`, `title: String`, `description: String`, `tags: Vec<InSpecTag>`, `tests: Vec<InSpecTest>`, `source_file: String`, `source_line: usize` | A parsed InSpec control block. Derives `Serialize`, `Deserialize`, `PartialEq`. |
| `InSpecTag` | `key: String`, `values: Vec<String>` | A tag on a control (e.g., `tag nist: ['AC-7']`, `tag severity: 'high'`). Key-values. |
| `InSpecTest` | `resource_type: String`, `resource_args: String`, `matchers: Vec<InSpecMatcher>` | A `describe` block. Resource type maps to ComplianceHelpers method. |
| `InSpecMatcher` | `property: Option<String>`, `expectation: String`, `negated: bool` | A matcher assertion (e.g., `its('Protocol') { should cmp 2 }`, `it { should be_installed }`). |

### Output Types (transpiler.rs)

| Type | Fields | Purpose |
|------|--------|---------|
| `RSpecFile` | `filename: String`, `content: String`, `control_id: String` | A generated RSpec test file ready for disk write. |

### Error Types (error.rs)

| Variant | Fields | When |
|---------|--------|------|
| `Parse` | `file`, `line`, `message` | Malformed InSpec control block |
| `Transpile` | `control_id`, `message` | Cannot convert a control to RSpec |
| `Io` | `path`, `source` | File/directory read/write failure |
| `InvalidProfile` | `path` | Not a valid InSpec profile directory |
| `NoControls` | `path` | Profile directory contains no control files |

## Parser (parser.rs)

The parser uses regex-based extraction rather than a full Ruby AST parser. InSpec controls follow a strict DSL that regex handles reliably:

1. `extract_control_id()` -- matches `control 'id' do` or `control "id" do`
2. `find_block_end()` -- tracks `do`/`end` depth to find matching `end`
3. `parse_control_block()` -- extracts impact, title, description, tags, describe blocks
4. `parse_describe_block()` -- extracts resource type, args, matchers
5. `parse_matcher()` -- extracts `its('prop') { should ... }` and `it { should ... }` patterns

The parser preserves:
- All NIST tags (`tag nist: ['AC-7', 'IA-5']`)
- All CIS tags (`tag cis: ['5.2.1']`)
- All severity tags (`tag severity: 'high'`)
- Impact scores (0.0 to 1.0)
- Source file and line number for traceability

## Transpiler (transpiler.rs)

`transpile_profile()` converts a list of `InSpecControl` values into `RSpecFile` values:

1. Each control becomes one `*_spec.rb` file
2. Resource types map to `ComplianceHelpers` methods:
   - `sshd_config` -> `ComplianceHelpers.read_config("/etc/ssh/sshd_config")`
   - `file` -> `ComplianceHelpers.file_content(path)`
   - `command` -> `ComplianceHelpers.command_output(cmd)`
   - `service` -> `ComplianceHelpers.service_status(name)`
   - `package` -> `ComplianceHelpers.package_info(name)`
   - `port` -> `ComplianceHelpers.port_info(port)`
   - `user` -> `ComplianceHelpers.user_info(name)`
3. NIST/CIS tags are preserved in RSpec metadata
4. Impact scores map to RSpec severity context

## Output (output.rs)

- `write_rspec_files()` -- writes `spec/compliance/*_spec.rb` files
- `write_helpers()` -- writes `spec/support/compliance_helpers.rb`
- `write_spec_helper()` -- writes `spec/spec_helper.rb` with profile name

## Helpers (helpers.rs)

`generate_helpers()` produces a `ComplianceHelpers` Ruby module that replaces InSpec's built-in resources:
- `read_config(path)` -- parses key-value config files (sshd_config, sysctl, YAML)
- `file_content(path)` -- reads file content
- `command_output(cmd)` -- executes shell command
- `service_status(name)` -- checks systemd service status
- `package_info(name)` -- checks package installation
- `port_info(port)` -- checks port listening status
- `user_info(name)` -- checks user existence

## Determinism Guarantee

The output is deterministic by construction:
1. Control files are sorted by filename before processing
2. Controls within each file are processed in source order
3. Tags, tests, and matchers preserve source order
4. No timestamps, random values, or environment-dependent content in output
5. The `hash` subcommand produces the same hash for the same input, every time

This is verified by integration test `integration_ssh_baseline_deterministic` which runs the pipeline twice and asserts hash equality.

## Test Evidence (94 tests)

| Property | Tests | Module |
|----------|------:|--------|
| InSpecControl serde roundtrip | 4 | `control.rs` |
| InSpecControl equality | 3 | `control.rs` |
| InSpecTag construction + variants | 4 | `control.rs` |
| InSpecTest resource types | 3 | `control.rs` |
| InSpecMatcher property/expectation/negated | 5 | `control.rs` |
| Parser: extract_control_id | 4 | `parser.rs` |
| Parser: find_block_end depth tracking | 3 | `parser.rs` |
| Parser: parse_controls single control | 4 | `parser.rs` |
| Parser: parse_controls multi-control file | 3 | `parser.rs` |
| Parser: impact extraction | 3 | `parser.rs` |
| Parser: title/description extraction | 3 | `parser.rs` |
| Parser: tag extraction (nist, cis, severity) | 6 | `parser.rs` |
| Parser: describe block extraction | 4 | `parser.rs` |
| Parser: matcher extraction (its, it, negated) | 5 | `parser.rs` |
| Parser: error handling (malformed blocks) | 3 | `parser.rs` |
| Transpiler: single control output | 3 | `transpiler.rs` |
| Transpiler: resource type mapping | 7 | `transpiler.rs` |
| Transpiler: tag preservation in RSpec metadata | 3 | `transpiler.rs` |
| Transpiler: impact to severity mapping | 2 | `transpiler.rs` |
| Transpiler: multi-describe blocks | 2 | `transpiler.rs` |
| Output: write_rspec_files creates files | 2 | `output.rs` |
| Output: write_helpers creates support file | 1 | `output.rs` |
| Output: write_spec_helper creates spec_helper | 1 | `output.rs` |
| Error type variants | 5 | `error.rs` |
| Integration: parse SSH baseline | 1 | `tests/transpile_test.rs` |
| Integration: transpile SSH baseline | 1 | `tests/transpile_test.rs` |
| Integration: write output files | 1 | `tests/transpile_test.rs` |
| Integration: deterministic hash | 1 | `tests/transpile_test.rs` |
| Integration: NIST tag preservation | 1 | `tests/transpile_test.rs` |
| Integration: CIS severity tag | 1 | `tests/transpile_test.rs` |
| Integration: all tests preserved | 1 | `tests/transpile_test.rs` |
| Integration: multi-describe parse | 1 | `tests/transpile_test.rs` |
| Integration: multi-describe transpile | 1 | `tests/transpile_test.rs` |
| Integration: full pipeline with helpers | 1 | `tests/transpile_test.rs` |
| **Total** | **94** | |

## Dependencies

| Crate | Purpose |
|-------|---------|
| clap | CLI argument parsing (derive) |
| serde + serde_json | Control serialization (inspect command) |
| regex | InSpec DSL parsing |
| chrono | Timestamps |
| thiserror | Error type derivation |
| tracing + tracing-subscriber | Structured logging |
| tempfile | Test isolation (dev-dependency) |

## Ecosystem Position

```
tameshi (core, 925 tests)
  +-- collectors/rspec.rs         -- hashes RSpec JSON output
  +-- collectors/inspec_result.rs -- hashes InSpec JSON output
  +-- certification_artifact.rs   -- binds compliance hash into 3-leaf Merkle

inspec-rspec (this repo, 94 tests)
  +-- parser.rs                   -- InSpec DSL -> InSpecControl types
  +-- transpiler.rs               -- InSpecControl -> RSpec test files
  +-- output.rs                   -- writes deterministic output
  +-- main.rs::run_hash()         -- computes attestation hash of output

inspec-akeyless (62 tests)
  +-- InSpec resource pack for Akeyless Vault
  +-- Transpiled by inspec-rspec for RSpec execution

pangea-architectures (118 tests)
  +-- RSpec synthesis tests verify IaC compositions
  +-- Results hashed by tameshi's RSpecResultCollector
```
