# 0135. The product is named Z3L

- Status: Accepted
- Date: 2026-11-11

## Context

The product carried a working name, `Z3L`, that described what it was for and nothing it
believed. The name was also hardcoded in fourteen places — the window title, the `run_native` app
name, an error sentence, the `.z3l` default and its filter, the settings directory in three platform
branches — so a name chosen after a release would have been a refactor instead of a constant.

A name had to satisfy four conditions at once: it must be clear in the software classes (Nice 9 and
42) where we would want it as a mark, it must read the same way to a Russian speaker and to an
English speaker, it must survive lowercasing into a binary, a crate and a directory, and it must not
already be a furniture brand.

Nineteen candidates were checked against the EUIPO, USPTO and WIPO registers, the domain registries
and the public stores; the measurements are in `plan/notes/R1.md`. Three findings shaped the search:
`Faber` is a live furniture configurator on `faber3d.com` and a class-9 word mark held by several
owners; `M3R` is a registered class-9 word mark (KMW Inc., protection including the EU and the US,
to 2031); `K3N` is a lubricants brand on `k3n.ru` since 2022.

## Decision

**The product is `Z3L`**, read as a word ("zel"), and it decodes as *Zero · 3 Laws*: `Local`,
`Literal`, `Lasting` — the three laws written in [`docs/philosophy.md`](../philosophy.md).

- The name itself is not in the code. It lives in one constant per surface — `theme::PRODUCT_NAME` for
  the words, an `APP_ID` for the window class and the settings directory, `project::EXTENSION` for the
  file — so the next rename is one line each and a test fails if a copy is missed.
- The crates stay unprefixed (`model`, `engine`, `render`), the application package is `app`, and the
  binary is `z3l`.
- The scene envelope is `"furniture.scene"`: the format string names a contract, not a brand, so it
  survives any future rename. The extension follows the brand: `.z3l`.
- `Faber` remains the assistant's name (ADR 0028) and is untouched: the assistant is a client of the
  command registry, not a second product.

## Consequences

- A person can now explain the name in one sentence, and the sentence is the philosophy rather than a
  story: the zero a part is measured from and the three laws its data obeys.
- The clearance is a check of public registers, not legal counsel. It is re-run before the product is
  published under the mark, and the first release is what makes the name expensive to change.
- Two further readings of the same string are recognised and recorded in `docs/philosophy.md`. They
  are not alternative brands: they are why the name can be adopted by a maker who never reads this
  record.
- The name search is a repeatable procedure with a recorded result; a future rename starts from
  `plan/notes/R1.md` instead of from a blank page.

## References

- [`docs/philosophy.md`](../philosophy.md) — the thesis, the three laws and the readings of the name.
- `plan/notes/R1.md` — the measured search: candidates, marks, domains, stores, ruled out roads.
- `plan/rename.md` — the record of the rename that landed: what the old name touched, the procedure as
  it was run and the gotchas that outlived it.
- ADR [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — `Faber`, the assistant.
- Plan: R1.
