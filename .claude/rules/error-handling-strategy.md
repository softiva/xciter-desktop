---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Error Handling Strategy

## Rules

- Define a small hierarchy of domain error types with a stable code and, for services, an HTTP status; do not throw bare generic errors.
- Handle errors in one central place (middleware or top-level handler) that maps them to responses and logs once.
- Never swallow errors; if you catch, recover, translate or rethrow with the cause.
- Make messages specific and safe: say what failed and what to do, never leak stack traces, queries or secrets to clients.
- Use a Result type for expected failures (validation, not found) and exceptions for unexpected ones.
- Retry only idempotent, transient failures, with a limit, backoff and jitter; never retry forever.
- Wrap UI subtrees in error boundaries so one failure does not blank the app.
