# Verification

## Build
`cargo build --locked`

## Unit tests
`cargo test --locked`
`py -3.12 -I tests/test_native_tools.py`

## Lint
`cargo clippy --all-targets --locked -- -D warnings`

## Format/check
`cargo fmt --check`

## Native source integration
`py -3.12 -I tools/native.py plan`
`py -3.12 -I tools/native.py check`
`py -3.12 -I tools/native.py build`
`py -3.12 -I tools/native.py test-net`
`py -3.12 -I tools/native.py verify`
Requires the exact source checkouts from compatibility.json. `check` uses no optional audio/controller features.
`test-net` uses real Horizon Session code and synthetic protocol fixtures without game resources.

## Native Rust formatting
`rustfmt --edition 2024 --check @(Get-ChildItem native -Filter '*.rs' | ForEach-Object FullName)`

## Browser/e2e / live gameplay / performance
None configured. Core tests do not prove native runtime support or performance.
