# Nyxis Governance

## Repository structure

```
github.com/nyxis-io/
├── nyxis/            BSL 1.1  — compiler, spec, CLI, conformance, benchmarks
├── nyxis-drivers/    MIT      — language SDKs (C, Go, Rust, Python, JS, Ruby, PHP, Kotlin, C#, Swift)
└── nyxis-extensions/ EULA     — private enterprise extensions
```

**`nyxis/`** is the authoritative source for the wire format specification (`SPEC.md`) and the reference Rust implementation. The spec itself is licensed for unrestricted implementation — see the Spec License section below.

**`nyxis-drivers/`** is MIT-licensed with no strings. Embed it in anything.

**`nyxis-extensions/`** contains proprietary components sold to enterprises. It depends on `nyxis/` via path import and adds capabilities on top of the public core; it contains no trade secrets that belong in the spec or the reference implementation.

---

## Spec License

The NXS wire format specification (`SPEC.md` and all conformance vectors in `conformance/`) is licensed under the **Creative Commons Attribution 4.0 International (CC BY 4.0)** license, separate from the BSL that governs the Rust implementation.

**This means:** anyone may implement the NXS wire format in any language, for any purpose, including commercial products, without a license from Micael Malta, provided they attribute the spec. This grant is irrevocable.

The BSL governs the *Rust compiler and tooling in this repository*, not the format itself. A clean-room implementation of the NXS wire format based solely on `SPEC.md` carries no BSL obligation.

---

## BSL free tier

The `nyxis/` repository is published under the **Business Source License 1.1**. The Additional Use Grant is:

> You may use the Software in production without a commercial license if your organization's gross annual revenue is below **$5,000,000 USD** and your aggregate monthly data volume processed by the Software is below **10 TB**.

Both conditions must hold. On **2029-05-20** this version of the code converts to the MIT license.

This threshold is set to cover individual developers, small teams, and growth-stage startups completely. If you are building a product and are unsure whether you qualify, you qualify.

---

## Commercial licensing

Organizations above either threshold purchase a commercial license via `COMMERCIAL.md`. Pricing is at `COMMERCIAL.md`; contact `licensing@nyxis.io` for procurement.

The drivers (`nyxis-drivers/`) are MIT and carry no commercial obligation at any scale.

---

## Contributor License Agreement

External contributions to `nyxis/` or `nyxis-drivers/` require a CLA assigning copyright to Micael Malta. This is required to preserve dual-licensing rights. The CLA is on file at `CONTRIBUTING.md`.

---

## Magic bytes and brand

| Identifier | Bytes |
|---|---|
| File header | `NYXB` `[0x4E 0x59 0x58 0x42]` |
| Object cell | `NYXO` `[0x4E 0x59 0x58 0x4F]` |
| List cell   | `NYXL` `[0x4E 0x59 0x58 0x4C]` |

File extensions: `.nxs` (source schema), `.nxb` (binary).

Prior to v1.0 (May 2026) the project used the working title "Nexus Standard." That name is retired. All magic bytes, extensions, and package identifiers use the Nyxis brand exclusively.

---

## Package namespaces

`nyxis` is reserved on Cargo, npm, PyPI, and `github.com/nyxis-io/nyxis-drivers/go`.

---

*Maintainer: Micael Malta — `licensing@nyxis.io`*
