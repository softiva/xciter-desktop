---
name: aicgen-security-auth-jwt
description: "Password hashing, JWT handling, login protection, sessions and authorization middleware. Use when implementing authentication, sessions or access control."
---

# Authentication & JWT Security

## Rules

- Hash passwords with a slow, salted algorithm: Argon2id preferred, or bcrypt with cost 12 or higher. Never use plain SHA or MD5, and never log or return passwords.
- Keep access tokens short-lived (about 15 minutes) and rotate refresh tokens; store refresh tokens server-side or as httpOnly, Secure, SameSite cookies, never in localStorage.
- Verify JWTs with a pinned algorithm allowlist, and check signature, `exp`, `iss` and `aud`; reject `alg: none`. Keep signing keys in a secret manager.
- Rate-limit and lock out login, password reset and token endpoints; return the same message for unknown user and wrong password.
- Authorize on the server for every request (role and ownership checks); never trust client-supplied roles or IDs.
- Regenerate the session ID on login, set cookie flags (`httpOnly`, `secure`, `sameSite`), and invalidate sessions on logout and password change.
