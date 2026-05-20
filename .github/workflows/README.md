# CI setup (private repositories)

Conformance jobs clone `nyxis-io/nyxis-drivers` to run language runners. Both org repos are **private**.

Add repository secret:

| Secret | Value |
|--------|--------|
| `NYXIS_CI_AUTOMATION_TOKEN` | PAT with **read** on `nyxis-io/nyxis-drivers` (and `nyxis-io/nyxis` if using cross-repo publish flows) |

Or enable org-level **Actions → Workflow permissions** so workflows in this org can read sibling repositories.
