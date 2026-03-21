# Security Policy

## Supported versions

Only the latest release is actively maintained. Security fixes are not backported to older versions.

| Version | Supported |
|---|---|
| Latest (`main`) | Yes |
| Older releases | No |

---

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

If you discover a security issue, report it privately using one of the following methods:

- **GitHub private vulnerability reporting** — use the [Security tab](https://github.com/okiedocus/gui_sdr_gps_sim/security/advisories/new) on the repository to submit a confidential advisory.
- **Email** — send a description to `info@okiedocus.nl` with the subject line `[SECURITY] gui_sdr_gps_sim`.

Please include:

- A description of the vulnerability and its potential impact.
- Steps to reproduce or a proof-of-concept (if applicable).
- The version or commit hash you tested against.
- Your preferred contact method for follow-up questions.

You can expect an acknowledgement within **5 business days** and a status update within **14 days** of the initial report.

---

## Scope

This project is a desktop application that generates GPS L1 C/A baseband signals. Security issues that are in scope include:

| Category | Examples |
|---|---|
| **Local privilege escalation** | The app running arbitrary code with elevated privileges |
| **Malicious file parsing** | A crafted RINEX, GPX, KML, or GeoJSON file causing memory corruption or code execution |
| **Network exposure** | The TCP server or UDP output inadvertently exposing attack surface on the local network |
| **Dependency vulnerabilities** | A published CVE in a direct dependency that affects this application |

Out of scope:

- Vulnerabilities in the HackRF hardware or its firmware.
- Issues that require physical access to the machine running the app.
- The legality of transmitting GPS signals — that is a legal matter, not a security one.
- Denial-of-service against the app itself by a local user (the threat model is a single-user desktop app).

---

## RF transmission warning

This application can transmit radio signals on the GPS L1 frequency (1 575.42 MHz). Unauthorised transmission of GPS signals is **illegal** in most jurisdictions and can interfere with safety-critical navigation systems.

- Only use this software in a properly shielded enclosure or with the appropriate regulatory licences.
- The maintainers will not accept contributions, issues, or requests that are intended to make unauthorised or harmful RF transmission easier.
- If you discover a way in which the software could be used to cause interference beyond the intended controlled-environment use case, please report it as a security issue using the process above.

---

## Dependency security

Dependencies are managed via `Cargo.lock`. To check for known vulnerabilities in the dependency tree:

```bash
cargo install cargo-audit
cargo audit
```

Pull requests that update dependencies to resolve a published CVE are welcome.

---

## Disclosure policy

Once a fix is available, the vulnerability will be disclosed publicly in a GitHub Security Advisory. Credit will be given to the reporter unless they prefer to remain anonymous.
