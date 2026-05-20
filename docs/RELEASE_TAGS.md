# Release tag prefixes

Registry publish workflows and matching CI jobs run when you push an annotated or lightweight tag whose name matches the pattern below. Each ecosystem is independent (no single repo-wide `v*` release trigger). In GitHub Actions, `on.push.tags` uses glob rules where `*` does not cross `/`, so workflows use a prefix + `**` (for example `rust/**`) instead of `rust/v*` so tags like `rust/v1.0.0` match reliably.

| Prefix      | Example      | Publish workflow (if any)   |
|------------|--------------|-----------------------------|
| `rust/v`   | `rust/v0.4.0` | `publish-crates.yml`        |
| `js/v`     | `js/v0.5.1`   | `publish-npm.yml`           |
| `python/v` | `python/v0.3.8` | `publish-pypi.yml`        |
| `go/v`     | `go/v0.4.3`   | CI only (`go.yml`)          |
| `ruby/v`   | …             | `publish-rubygems.yml`      |
| `php/v`    | …             | `publish-packagist.yml`     |
| `csharp/v` | …             | `publish-nuget.yml`         |
| `kotlin/v` | …             | `publish-kotlin.yml`        |
| `c/v`      | …             | `publish-c-release.yml` (tarball + GitHub Release) |
| `swift/v`  | …             | CI only (`swift.yml`)       |

All of these workflows also support **`workflow_dispatch`** for manual runs.

**Go:** The Go toolchain’s module proxy expects semver tags that match your module layout (often plain `vX.Y.Z` at the module root). The `go/v…` tags here are for CI (and any future automation you add); they do not replace whatever tags you use for `go get` if those differ.
