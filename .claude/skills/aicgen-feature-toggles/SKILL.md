---
name: aicgen-feature-toggles
description: "Release, experiment and ops toggles and how to implement and retire them. Use when adding a feature flag or gradual rollout."
---

# Feature Toggles

## Rules

- Classify each toggle (release, experiment, ops, permission) and give each a name, owner and expiry.
- Remove release toggles promptly after full rollout; stale toggles are technical debt.
- Keep toggle checks at the edges of the code (one decision point), not scattered through business logic.
- Default to off, test both states, and make toggles changeable without a deploy.
- Never use a toggle as a security control or ship unfinished work behind one without tests.

## Toggle Types

### Release Toggles
Hide incomplete features in production.

### Experiment Toggles
A/B testing and gradual rollouts.

### Ops Toggles
Runtime operational control.

## Best Practices

- Remove toggles after feature is stable
- Use clear naming conventions
- Log toggle decisions for debugging
- Test both toggle states
- Limit number of active toggles
- Document toggle purpose and expiration
