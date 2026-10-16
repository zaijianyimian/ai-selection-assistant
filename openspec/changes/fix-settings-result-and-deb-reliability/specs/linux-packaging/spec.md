# Spec Delta

## Purpose

Make Linux installation compatibility inspectable before application startup, so Debian package metadata accurately describes the runtime requirements of the delivered executable and published support claims match recorded validation.

## ADDED Requirements

### Requirement: DEB declares executable runtime version requirements
Every released DEB MUST declare version constraints sufficient for the shared-library symbols required by its final executables, including the required libc6 version. Runtime-library resolution MUST be checked against the build environment's package metadata. The package MUST retain required clipboard tools, Chinese fonts and desktop integration assets.

#### Scenario: Executable requires GLIBC_2.39
- **WHEN** the final executable imports GLIBC_2.39 symbols
- **THEN** the generated DEB declares libc6 at least 2.39 or a stricter minimum derived from the package metadata, rather than allowing an older glibc silently

#### Scenario: Required constraint is removed
- **WHEN** a candidate package omits or weakens a required runtime constraint
- **THEN** the packaging verification step fails and that candidate is not reported as release-ready

#### Scenario: Target runtime is older than required
- **WHEN** dependency resolution is evaluated against a system whose libc6 is older than the declared requirement
- **THEN** the package manager refuses installation or requires a compatible runtime upgrade before installation

### Requirement: Published compatibility is tied to the artifact
Release documentation MUST identify the artifact architecture, build system and minimum runtime dependencies, distinguish source-build guidance from prebuilt-package validation, and identify any untested installation or platform checks. The release process MUST NOT claim compatibility from build success alone.

#### Scenario: Package built on the current host
- **WHEN** an amd64 DEB is produced on Ubuntu 26.04
- **THEN** its documentation records that build environment and verified runtime floor without claiming that all older Ubuntu source-build targets can run that same binary

#### Scenario: Supported-environment installation check
- **WHEN** a package is marked as installed and launched successfully on a target environment
- **THEN** an actual installation and startup result is recorded; dependency simulation alone is labelled as simulation

#### Scenario: Runtime validation is unavailable
- **WHEN** a required clean-target or older-runtime check cannot be run
- **THEN** it is reported as blocked or unverified and is not recorded as passed
