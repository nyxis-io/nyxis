---
room: registry
subdomain: rust
source_paths: [rust/src/registry/]
see_also: ["bins.md", "compiler_pipeline.md", "tests_fuzz.md"]
hot_paths: [rust/src/registry/client.rs, rust/src/registry/preamble.rs]
architectural_health: normal
security_tier: sensitive
---

# rust/ — Schema Registry Client

Subdomain: rust/
Source paths: rust/src/registry/

## TASK → LOAD

| Task | Load |
|------|------|
| Connect to registryd over gRPC | registry.md |
| Register or fetch schema by DictHash | registry.md |
| Parse/format 0x-prefixed dict hashes | registry.md |

---

# registry/client.rs

DOES: Async tonic client for RegistryService (register, list, get-by-hash) with connect/timeouts.
SYMBOLS:
- RegistryClient { inner RegistryServiceClient }
- connect(server &str) → Result<Self, String>
- register_schema(&mut self, dict_hash, schema_bytes, drift_policy) → Result<RegisterSchemaResponse, String>
- list_schemas, get_schema_by_hash
DEPENDS: registry/pb (tonic include_proto)
CONFIG: registry server URL (http/https)
PATTERNS: grpc-client

---

# registry/mod.rs

DOES: Registry module root; re-exports client/preamble; includes generated nyxis.registry.v1 protobuf; dict hash hex parse/format helpers.
SYMBOLS:
- parse_dict_hash_hex(s &str) → Result<[u8; 8], String>
- format_dict_hash(bytes &[u8; 8]) → String
- pb module (tonic include_proto)

---

# registry/preamble.rs

DOES: Builds registry registration payloads from compiled schema bytes and drift policy metadata.
SYMBOLS:
- (+preamble serialization helpers for RegisterSchemaRequest)
