---
name: aicgen-security-injection
description: "Preventing SQL, NoSQL, command and path-traversal injection with input validation. Use when handling user input, building queries or running shell commands."
---

# Injection Prevention

## Rules

- Never build SQL, shell commands, queries or HTML by concatenating user input; use parameterized queries, argument arrays and context-aware escaping.
- Validate input at the boundary with an allowlist (type, length, format); validation supplements, and never replaces, parameterization.
- Do not pass user-controlled objects into NoSQL queries; coerce to expected primitive types first.
- Avoid shell execution; if unavoidable, call the program directly with an argument array and no shell.
- Resolve file paths against a fixed base directory and reject any result that escapes it.
- Use least-privilege database accounts so an injection has limited impact.

## Rust: Injection Prevention

```rust,ignore
// ❌ DANGEROUS: formatting user input into SQL
let sql = format!("SELECT * FROM users WHERE email = '{email}'");

// ✅ SAFE: bound parameters (sqlx)
let user = sqlx::query_as!(User, "SELECT id, email FROM users WHERE email = $1", email)
    .fetch_optional(&pool)
    .await?;

// ❌ DANGEROUS: shell with interpolated input
std::process::Command::new("sh").arg("-c").arg(format!("convert {filename} out.jpg"));

// ✅ SAFE: no shell; each argument is passed separately
std::process::Command::new("convert").arg("--").arg(&filename).arg("out.jpg").status()?;

// ✅ SAFE: path traversal check before touching the filesystem
let full = base_dir.join(&requested).canonicalize()?;
if !full.starts_with(&base_dir) {
    return Err(ApiError::Forbidden);
}
```
