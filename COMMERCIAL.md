# Nyxis Commercial Licensing & Support

## Repositories

| Repository | License | Role |
| --- | --- | --- |
| [`nyxis`](https://github.com/nyxis-io/nyxis) | BSL 1.1 | Spec (`SPEC.md`), Rust compiler/CLI, conformance, benchmarks |
| [`nyxis-drivers`](https://github.com/nyxis-io/nyxis-drivers) | MIT | Language SDKs — embed without copyleft |
| [`nyxis-extensions`](https://github.com/nyxis-io/nyxis-extensions) | Commercial EULA | Private enterprise components (depends on `nyxis` only) |

The NXS wire format spec (`SPEC.md`, conformance vectors) is **CC BY 4.0** — anyone may implement the format with attribution. The BSL governs this repository’s Rust tooling, not clean-room spec implementations.

Drivers are MIT at any scale. Extensions add enterprise capabilities; they must not move trade secrets into the public spec or core.

External contributions to `nyxis/` or `nyxis-drivers/` require the CLA in [CONTRIBUTING.md](./CONTRIBUTING.md).

---

## BSL free production tier

Production use of `nyxis/` without a commercial agreement is allowed when **both** are true:

- Gross annual revenue is **under $5,000,000 USD** (global org including affiliates under common control), **and**
- Aggregate data processed by the Licensed Work is **under 10 terabytes (TB) per calendar month**.

If either limit is exceeded, you need a commercial license below. Non-production use (dev, test, benchmark, research) is permitted under BSL 1.1 without those caps. This codebase converts to **MIT on 2029-05-20** per [LICENSE](./LICENSE).

---

## Commercial packages

Organizations above the BSL free tier purchase a commercial production license. We map packages to operational scale:

### 1. Startup / Growth Tier — $3,500 / year

*For growing companies or moderately sized data operations.*

* **Eligibility Thresholds:** * Your organization's gross annual revenue is **under $10,000,000 USD**; AND
  * Total production data processed by Nyxis is **under 1 Terabyte (TB) per month**.
* **Included Features:**
  * Commercial closed-source production license clearance.
  * Access to all core v1.0 libraries and 10-language reference readers.
  * Email-based bug and incident reporting.

### 2. Enterprise Core Tier — $15,000 / year

*For established corporate architectures, high-volume IoT networks, or trading infrastructure.*

* **Eligibility Thresholds:** * Your organization's gross annual revenue is **between $10,000,000 and $100,000,000 USD**; OR
  * Total production data processed by Nyxis is **between 1 Terabyte (TB) and 50 Terabytes (TB) per month**.
* **Included Features:**
  * Everything in Startup Tier.
  * **Nyxis In-Memory Compactor Daemon** (`nxs-compactd`) — sealed-segment repack and slack reclamation.
  * **Encrypt-at-rest v1** (`nyxis-encrypt`) — AES-256-GCM segment sealing with envelope sidecars; file-based DEK via `NYXIS_ENCRYPT_KEY_PATH` (KMS plugins: Tier 5 backlog).
  * **Segment replication v1** (`nxs-replicate`) — `plan` + `apply` for `file://` destinations; manifest idempotency and encrypt sidecar copy.
  * **Read-only query proxy v1** (`nxs-queryd`) — `AggregateSum`, streaming `ScanColumn`, Prometheus `/metrics`, optional registry validation and encrypted-segment decrypt (bounded by `NYXIS_QUERY_MAX_SEGMENT_BYTES`).
  * Dedicated Slack support channel with Next-Business-Day SLA.

### 3. Principal Tier — Custom Quote

*For massive telemetry platforms, high-volume trading desks, and hyper-scale architectures.*

* **Eligibility Thresholds:** * Your organization's gross annual revenue exceeds **$100,000,000 USD**; OR
  * Total production data processed by Nyxis exceeds **50 Terabytes (TB) per month**.
* **Included Features:**
  * Unlimited data ingestion volume and server instances.
  * **Nyxis-to-Apache Arrow Zero-Copy Memory Bridge**.
  * **Distributed Schema Registry & Proxy** (`nxs-registryd`) — gRPC/REST validation and additive-only drift resolution MVP ([`nyxis-extensions`](../nyxis-extensions/)).
  * **Hardware-Accelerated SIMD Guard** (`nyxis-simd-guard`) — dense batch kernels (MVP correctness path; AVX-512 ≤1.5× Arrow Workload&nbsp;C is a **manual** gate on bench hardware, not a blanket production SLA).
  * 4-hour critical incident response SLA and legal indemnification.

To purchase a license or request a custom enterprise evaluation contract, reach out directly to **licensing@nyxis.io**.
