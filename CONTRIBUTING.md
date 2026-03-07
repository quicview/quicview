# Contributing

Thank you for your interest in contributing to QuicView!

## Getting Started

```bash
git clone https://github.com/quicview/quicview.git
cd quicview
cargo build --workspace
cargo test --workspace
```

## Guidelines

- Follow Rust 2024 edition idioms
- Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` before submitting
- Add tests for new functionality
- Use conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `ci:`, `build:`

## License

By contributing, you agree that your contributions will be licensed under the MIT OR Apache-2.0 dual license.
