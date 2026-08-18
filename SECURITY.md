# Security

## Reporting a vulnerability

Please use GitHub's [private vulnerability
reporting](https://github.com/swalla02/noidroid/security/advisories/new) rather than a
public issue. Include what you did, what happened, and what you expected. Expect an
acknowledgement within a week.

## What is in scope

Paranoid Android runs other people's programs and stores what they did, so the
security-relevant surface is mostly about containment and integrity:

- **An effect firing when it should not.** A replay must never perform a mediated
  interaction, and an effect declared `irreversible` must never be performed outside
  an original recording. A way to make either happen is a vulnerability, not a bug.
- **A branch reaching outside its own workspace**, or altering its parent's objects.
- **A recorded trajectory being altered without detection.** `noidroid verify` should
  catch any object whose bytes no longer match its name.
- **A recording leaking secrets in a surprising place.** Recordings deliberately
  contain everything an execution saw: tool responses, HTTP bodies, file contents,
  screenshots. That is the point, and `.noidroid/` should be treated as sensitive.
  But a path that copies that data somewhere the operator would not expect is in
  scope.

## What is not in scope

- A recorded trajectory containing secrets that the recorded program handled. Treat
  `.noidroid/` as you would treat a heap dump.
- Running an untrusted program under `noidroid run`. The engine does not sandbox the
  program it records; the workspace is for reproducibility, not isolation.
- The browser adapter reaching the network when explicitly allowed to.
