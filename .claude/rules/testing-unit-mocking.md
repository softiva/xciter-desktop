---
paths:
  - "**/*.rs"
---

# Test Doubles and Mocking

## Rules

- Mock only boundaries you do not own or that are slow or non-deterministic: network, database, clock, filesystem, email. Do not mock the code under test or simple value objects.
- Pick the lightest double: stub for canned answers, fake for working in-memory behavior, spy or mock only when the interaction itself is the behavior.
- Verify interactions sparingly; prefer asserting on the result.
- Reset or recreate doubles between tests so state cannot leak.
- If a test needs many mocks, the unit has too many dependencies; simplify the design instead.
