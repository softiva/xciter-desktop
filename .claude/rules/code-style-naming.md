---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Naming Conventions

## Rules

- Follow the language's convention and the project's existing style first; consistency beats preference.
- Names say what a thing is or does: nouns for values and types, verbs for functions, `is/has/can` for booleans.
- Avoid abbreviations, single letters (except tiny loop indexes) and vague words like `data`, `tmp`, `manager`, `util`.
- Replace magic numbers and strings with named constants.
- Name files after the main thing they export, in the language's usual case.
