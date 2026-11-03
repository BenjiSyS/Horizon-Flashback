# Verification

## Build
`cargo build --locked`

## Unit tests
`cargo test --locked`

## Lint
`cargo clippy --all-targets --locked -- -D warnings`

## Format/check
`cargo fmt --check`

## Native source integration
`py -3.12 -I tools/native.py plan`
`py -3.12 -I tools/native.py check`
`py -3.12 -I tools/native.py build`
Requires the exact source checkouts from compatibility.json. `check` uses no optional audio/controller features.

## Native Rust formatting
`rustfmt --edition 2024 --check @(Get-ChildItem native -Filter '*.rs' | ForEach-Object FullName)`

## Browser/e2e / live gameplay / performance
None configured. Core tests do not prove native runtime support or performance.
