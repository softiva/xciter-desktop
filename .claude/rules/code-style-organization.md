---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Code Organization

## Rules

- Keep functions short and single-purpose (aim under ~30 lines); extract when a function needs a comment to explain a section.
- Limit nesting to about three levels using early returns and guard clauses.
- Keep files focused (about 200-300 lines); split by responsibility, not by type.
- Order a file consistently: imports, constants, types, main code, helpers.
- Give each class or module one reason to change.
- Remove duplication when the same logic appears three times, but do not abstract prematurely.
