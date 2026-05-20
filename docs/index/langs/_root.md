---
room: _root
subdomain: langs
source_paths: ruby/, php/, kotlin/, csharp/, swift/
see_also: docs/index/_root.md
architectural_health: normal
security_tier: normal
---

# Languages — Building Router

Subdomain: langs/
Source paths: ruby/, php/, kotlin/src/main/kotlin/nxs/, csharp/, swift/Sources/

## TASK → LOAD

| Task | Load |
|------|------|
| Read or write .nxb from Ruby | ruby.md |
| Use the Ruby C extension | ruby.md |
| Read or write .nxb from PHP | php.md |
| Use the PHP C extension | php.md |
| Read or write .nxb from Kotlin/JVM | kotlin.md |
| Read or write .nxb from C# (.NET) | csharp.md |
| Read or write .nxb from Swift | swift.md |
| Benchmark any of these language implementations | (see individual room) |

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| ruby.md | ruby/*.rb, ruby/ext/nxs/ | 8 |
| php.md | php/*.php, php/nxs_ext/ | 8 |
| kotlin.md | kotlin/src/main/kotlin/nxs/ | 5 |
| csharp.md | csharp/ | 5 |
| swift.md | swift/Sources/, swift/Package.swift | 6 |
