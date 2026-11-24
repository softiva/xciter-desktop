---
paths:
  - "**/*.rs"
---

# Unit Testing Fundamentals

## Rules

- Use Arrange-Act-Assert and keep one concept per test.
- Assert on observable behavior (return values, state, outputs), not on which internal methods were called.
- Name tests by behavior and condition (`rejects an expired token`), not by method name or `works`.
- Each test creates its own data; never rely on another test's side effects or execution order.
- Cover boundaries and failure inputs: zero, empty, max, invalid, and error paths.
