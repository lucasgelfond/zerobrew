# Contributing to zerobrew

Thanks for your interest in contributing to zerobrew! This document provides guidelines for contributing to the project.

## Soft Prerequisites

- Rust 1.90 or later
- Access to either a macOS or Linux machine

## A note on LLM usage
While we encourage the use of LLM's for thinking through problems, helping with tests, and even writing code, we simply 
cannot accept or tolerate PRs with no clear guidance or thought put into them.

**_Please understand_** that we reserve the right to simply close your PR if it exhibits clear indicators 
of heavy LLM usage. We understand you are excited to contribute but the code must reach a level of quality
that's typically achieved through thoughtful engagement in the community and the issues/agenda of zerobrew- NOT
by throwing a prompt into an LLM and opening a PR with no direction.

If you ever need help or want to walk through an issue or idea that you have with one of the maintainers, feel free to join 
the [community discord](https://discord.gg/TVatsQBFJt); we would be more than happy to assist you.

## Project Structure

zerobrew is organized as a Cargo workspace with three crates:

- `zb_core`: Core data models and domain logic (formula resolution, bottle selection)
- `zb_io`: I/O operations (API client, downloads, extraction, installation)
- `zb_cli`: Command-line interface

Any changes you make that touch several crates should be organized properly. See [commit hygiene](#commit-hygiene)

## General Development Workflow

1. Fork the repo and create a new branch for your changes:
   ```bash
   git checkout -b feat/something
   ```

2. Make your changes and ensure, at the _least_:
   - Code is formatted: `cargo fmt --all`
   - No clippy warnings: `cargo clippy --workspace --all-targets -- -D warnings`
   - Tests pass: `cargo test --workspace`
> [!NOTE] 
> These will run in CI but it's best you clean up your code _before_ opening a PR to ensure a quick 
> turnaround!

3. Write tests for new functionality. Each module should have accompanying tests.

4. Commit your changes with clear, descriptive commit messages (see below)

5. Push to your fork and submit a pull request.

## Commit hygiene
We ask that you follow the format below for commits:
```bash
[fix / feat]($crate): description
```

for instance:
```bash
fix(zb_cli): foo bar moo baz
```
Generally speaking, we also ask that you please write isolated, [atomic commits](https://en.wikipedia.org/wiki/Atomic_commit). 
This means if you are approaching a PR that touches various parts of the codebase for example, ensure that your commits
are contained and cleanly seperated, properly describing/notating which commits belong where.


## Testing

- Unit tests should be colocated with the code in `mod tests` blocks
- Use `tempfile` for filesystem tests
- Use `wiremock` for HTTP mocking in integration tests
- Tests should be deterministic and not rely on external network access

## Running Benchmarks

To benchmark performance:

```bash
./benchmark.sh
```

This runs a 100-package installation suite comparing zerobrew to Homebrew. This is especially crucial to run if you are 
planning on contributing to performance/optimization related changes.

## Questions?

For further questions, open an issue on GitHub.

