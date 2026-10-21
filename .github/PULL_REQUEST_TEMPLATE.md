## What this changes

<!-- What does this PR do, and why? Link the issue it fixes: "Fixes #123". -->

## How it was tested

<!-- Tests added, and anything you checked by hand (keys pressed, screens, OS). -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `pnpm typecheck` passes (in `apps/desktop`)
- [ ] `pnpm strings --missing` lists no new strings (in `apps/desktop`)
- [ ] Every new UI string goes through `t()`, and has its Arabic in `src/locales/ar.ts` (marked `// ar: check` if unsure); layout uses CSS logical properties
- [ ] Works from the keyboard, with no TallyPrime key broken (or the change is explained above)
- [ ] Money uses `rust_decimal`; changes to the books go through `Book::apply_entry`; country rules stay in a compliance pack
- [ ] Tax rules: each rule, rate or threshold links its **primary official source** in the code or docs (or: no tax rules changed)
- [ ] No real company data, customer data or credentials in code, tests or screenshots
- [ ] Clean-room: nothing copied from TallyPrime or another product
- [ ] Every commit is signed off (`git commit -s`, [DCO](https://developercertificate.org))
