<!-- Based on the GitHub security-policy guidance: https://docs.github.com/en/code-security/getting-started/adding-a-security-policy-to-your-repository -->

# Security policy

Found a way to make Codefang misbehave that goes beyond a normal bug? We want to
hear about it privately first, so we can fix it before it gets weaponised.

## Supported versions

Codefang is pre-1.0 and ships no tagged releases yet, so the supported surface is
simple: we patch the tip and we do not backport.

| Version | Supported |
|---|---|
| Latest `main` (most recent release line) | Yes |
| Older pre-release commits | No |

If you are running an older commit, the first fix is to update to the latest
`main` and confirm the issue still reproduces.

## Reporting a vulnerability

Please **do not open a public issue, discussion, or pull request** for a security
vulnerability. A public report tells attackers about the hole before we can patch
it.

Instead, use GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab.
2. Choose **Report a vulnerability** to open a private
   [GitHub Security Advisory](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability).
3. Include:
   - A description of the vulnerability and the impact you think it has.
   - Reproduction steps or a proof-of-concept.
   - The affected commit, branch, or build.

This channel is private between you and the maintainers until a fix is ready.

## What happens next

- We aim to acknowledge your report within a few business days.
- Once confirmed, we work on a fix and keep you posted as it lands on `main`.
- We credit reporters in the advisory and release notes unless you ask us not to.

Thanks for helping keep Codefang and the people who run it safe.
