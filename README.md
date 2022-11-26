# gh-backup

> Blazingly fast tool to backup a GitHub organisation

This tool creates a backup for a GitHub organisation. It clones the repositories if they do not exist in the backup.
If they already exist, it executes a `git fetch`.

The tool will perform clones and fetches in parallel. Downloading a 10GB GitHub organisation takes only 2 minutes.

## Install

```
cargo install gh-backup
```

## Usage

```
export GH_TOKEN=ghp_
gh-backup some_org_xyz
```

## Building

```
cargo build
```
