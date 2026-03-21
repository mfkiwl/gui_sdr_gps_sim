## What does this PR do?

<!-- A concise description of the change and why it is needed. Link the related issue if there is one. -->

Closes #

---

## Type of change

<!-- Check all that apply. -->

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor (no behaviour change)
- [ ] Documentation
- [ ] CI / build
- [ ] Dependency update

---

## How was this tested?

<!-- Describe how you verified the change works. Include hardware used if relevant. -->

- [ ] `bash check.sh` passes locally (fmt, clippy, tests)
- [ ] Tested on Windows / Linux / macOS _(delete as applicable)_
- [ ] Tested with HackRF One connected
- [ ] Tested with simulated output (IQ file / UDP / Null)
- [ ] Not applicable — documentation or CI only change

---

## Checklist

- [ ] Code is formatted (`cargo fmt --all`)
- [ ] No Clippy warnings (`cargo clippy --all-features -- -D warnings`)
- [ ] No `unwrap()`, `todo!()`, `println!()`, or wildcard imports introduced
- [ ] New lints suppressed with `#[expect(lint, reason = "…")]`, not `#[allow]`
- [ ] PR is focused on a single concern — unrelated fixes are in separate PRs

---

## Screenshots or output

<!-- If this changes the UI or simulator output, add a screenshot or sample log. Delete this section if not applicable. -->
