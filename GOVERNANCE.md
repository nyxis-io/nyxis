# Nyxis Project Architecture, Licensing, and Commercialization Blueprint

This document defines the formal engineering architecture, licensing boundaries, registry strategies, and commercial monetization tiers for **Nyxis** (`github.com/nyxis-io`). All team members, contributors, and core maintainers must strictly adhere to these guidelines to preserve the project's intellectual property (IP) and ensure seamless commercialization.

---

## 1. Executive Summary & Brand Realignment

**Nyxis** (pronounced *"NIX-iss"*, derived from the Greek *νύξις*, meaning "surgical piercing") is an ultra-low-latency, memory-aligned binary data serialization contract and codec infrastructure. It replaces the legacy, congested brand identity "Nexus Standard" to eliminate naming collisions with decades of historical academic, scientific, and phylogenetic data formats.

### Core Metaphor & Value Proposition

* **Traditional Formats (JSON/Protobuf):** High overhead parsing. They act like a shredder, forcing the system to unpack, decode, and allocate heap memory for entire message blocks just to read a single field.
* **The Nyxis Approach:** Surgical data access. Utilizing strict 8-byte CPU-native boundaries, a centralized schema contract, and a trailing presence bitmask/local offset index, Nyxis enables constant-time $O(1)$ direct register lookups. The CPU register "pierces" directly into a binary payload (`.nxb`) to grab values via pointer casting without a single deserialization pass.

---

## 2. Multi-Repository Operational Blueprint

To maximize developer adoption while establishing un-bypassable legal tollbooths for enterprise revenue, the Nyxis ecosystem is split across three distinct repositories within the `github.com/nyxis-io` organization.

```text
github.com/nyxis-io/
├── nyxis/                 [ PUBLIC - BSL 1.1 ] Main compiler, spec, CLI, and core engine.
├── nyxis-drivers/         [ PUBLIC - MIT ] Multi-language client SDK readers and writers.
└── nyxis-extensions/      [ PRIVATE - Closed Source ] Premium enterprise components.
```

### Repo A: `nyxis-io/nyxis`

* **Visibility:** Public
* **License:** Business Source License 1.1 (BSL 1.1)
* **Contents:** The core serialization specification document (`SPEC.md`), the architecture manifesto (`MANIFESTO.md`), the core schema compiler/parser, the single-record CLI encoding engine, and the main system binaries.
* **Role:** Acts as the primary developer entry point. It represents a fully functional, complete engine so that engineers can test and build local infrastructure prototypes without feeling restricted by a "bare-bones" core.

### Repo B: `nyxis-io/nyxis-drivers`

* **Visibility:** Public
* **License:** MIT License (or Apache 2.0)
* **Contents:** The multi-language client Software Development Kits (SDKs) and runtime libraries (including Rust, Go, Python, TypeScript, C, and Java reference readers/writers).
* **Role:** Eliminates corporate "license contamination" fears. Corporate attorneys strictly prohibit embedding copyleft or source-available code inside client-facing apps. By placing drivers under the permissive MIT license, companies can freely embed Nyxis encoders into edge devices, mobile apps, and distributed microservices with zero legal friction. This drives massive organic adoption and network effects.

### Repo C: `nyxis-io/nyxis-extensions`

* **Visibility:** Private (Closed-Source)
* **License:** Proprietary Commercial EULA (End User License Agreement)
* **Contents:** Advanced enterprise scale machinery, data lifecycle utilities, and high-performance analytical integrations.
* **Role:** The primary driver of corporate subscription revenue. Access is strictly granted via machine tokens or GitHub team access upon execution of a paid commercial contract.

---

## 3. Core Technical Specifications & Magic Bytes

All reference implementations must transition to the new brand markers to maintain legal isolation from historical open-source codebases.

### 1. File Extensions

* **`.nxs`:** Human-readable plain-text source schema and configuration files utilizing custom text sigils.
* **`.nxb`:** Memory-aligned, zero-copy machine-optimized binary assets ready for direct pointer casting.

### 2. Magic Preamble Byte Changes

To ensure strict protocol enforcement and clean validation hooks inside network streams, header signatures are defined as follows:

| Layer | Old Specification Identifier | New Nyxis Identifier | Byte Array Representation |
| --- | --- | --- | --- |
| **Binary Layout Contract** | `NYXB` | **`NYXB`** | `[0x4E, 0x59, 0x58, 0x42]` |
| **Object Core Layout** | `NYXO` | **`NYXO`** | `[0x4E, 0x59, 0x58, 0x4F]` |
| **List Layout** | `NYXL` | **`NYXL`** | `[0x4E, 0x59, 0x58, 0x4C]` |

---

## 4. Licensing Engine & Hard Commercial Thresholds

The core repository (`nyxis-io/nyxis`) relies on the **Business Source License 1.1 (BSL 1.1)**. This license provides public visibility and community trust while establishing objective production parameters that force paid corporate upgrades.

### The BSL 1.1 Configuration Parameters

The parameters inside the root `LICENSE` are explicitly declared as:

* **Licensor:** Micael Malta
* **Software:** Nyxis Core (including all tools within this repository)
* **Change License:** MIT License
* **Change Date:** 2029-05-20 (Strict 3-year rolling chronological delay)
* **Additional Use Grant:** You are licensed to use the Software for any Production Use, provided that Your organization (including all global parent companies, affiliates, and subsidiaries under common control) fully complies with BOTH of the following conditions:
  1. Your organization's gross annual revenues are less than **$1,000,000 USD**; AND
  2. The total aggregate volume of data processed, serialized, or deserialized by the Software across your entire production infrastructure does not exceed **100 Gigabytes (GB)** within any single calendar month.

### The Rolling Mechanism

On May 20, 2029, the specific version of the code pushed today automatically flips to the MIT license. However, all new feature upgrades, performance optimizations, and versions pushed in subsequent years maintain their own rolling 3-year commercial BSL protection window.

---

## 5. Commercial Packages & Monetization Strategy

When an enterprise crosses either of the BSL gates ($1M revenue OR 100GB/month throughput), their right to use the core software under the BSL terminates. They must purchase a commercial subscription defined in `COMMERCIAL.md`.

Pricing is value-based, anchored directly to the cloud compute savings achieved by eliminating the "parsing tax."

### Tier 1: Startup / Growth Package

* **Price:** $3,500 / year (Standard corporate credit card profile)
* **Eligibility Gates:** Gross annual revenue <$10M USD **AND** production data throughput <1 Terabyte (TB) per month.
* **Included Scope:** Commercial closed-source production license clearance; access to all core v1.0 compilers; access to 10-language MIT drivers; standard email-based incident reporting.

### Tier 2: Enterprise Core Package

* **Price:** $15,000 / year (Standard procurement purchase order)
* **Eligibility Gates:** Gross annual revenue between $10M and $100M USD **OR** production data throughput between 1 TB and 50 TB per month.
* **Included Scope:** Everything in the Startup Tier plus:
  * **The Nyxis In-Memory Compactor Daemon:** A private, highly-threaded system utility that scans, defragments, and packs `.nxb` data sectors on the fly, reducing storage footprints by an extra 20-30% without halting active read/write streams.
  * **Dedicated Support SLA:** A private corporate Slack channel with guaranteed Next-Business-Day engineering response.

### Tier 3: Principal / Custom Package

* **Price:** $50,000+ / year (Custom Enterprise Service Contract)
* **Eligibility Gates:** Gross annual revenue >$100M USD **OR** production data throughput >50 TB per month.
* **Included Scope:** Unlimited data ingestion volume and server instances plus:
  * **The Nyxis-to-Apache Arrow Zero-Copy Memory Bridge:** A proprietary component that maps streaming `.nxb` fields directly into Apache Arrow memory allocations via bitwise shifting, instantly feeding Snowflake and Tableau ETL layers without deserialization.
  * **Distributed Schema Registry & Proxy:** Enterprise server application providing real-time schema validation and automatic runtime schema drift resolution across multi-cloud environments.
  * **Hardware-Accelerated SIMD Guard:** Extensions leveraging AVX-512 or ARM NEON instruction sets for batch field calculations.
  * **Legal Indemnification:** Full corporate IP indemnification and 4-hour critical production incident SLA.

---

## 6. Global Package Registry Strategy

To prevent namespace hijacking, the project founder must immediately lock down the raw identifier **`nyxis`** across all major programming ecosystem package managers before public repo disclosure.

* **Cargo (Rust):** `nyxis`
* **npm (Node.js):** `nyxis`
* **PyPI (Python):** `nyxis`
* **Go Modules:** `github.com/nyxis-io/nyxis-drivers/go` (internal package: `nyxis`)

---

## 7. Legal Guardrails & IP Protection

1. **Copyright Structuring:** All copyright notices across files, headers, and docs must be held in the founder's personal name, **not** the brand name. Format:
   `Copyright (c) 2026-Present Micael Malta. All rights reserved.`
   This ensures the individual creator retains absolute ownership of the core IP.
2. **Contributor License Agreements (CLAs):** No external code contributions may be merged into `nyxis` or `nyxis-drivers` unless the author signs a CLA assigning their copyright to Micael Malta. Without this, external open-source contributions will legally block the founder's right to dual-license the code and monetize the enterprise tier.
3. **Trade Secret Preservation:** Under no circumstances should the source code for the background compactor daemon, the SIMD vector engines, or the Apache Arrow zero-copy memory bridge be pushed to public repositories. These are classified as proprietary trade secrets and must remain isolated inside the private `nyxis-extensions` repository.

---

*Document Version: 1.0.0* · *Authorized for distribution within the Nyxis core engineering group.*
