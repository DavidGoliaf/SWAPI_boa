# WORK ORDER: T-01 — Workspace skeleton, toolchain, CI

| Metadata | Value |
|---|---|
| **Task id** | `T-01-WORKSPACE-SKELETON` |
| **Milestone** | M0 |
| **Target crates** | workspace root + seven crate stubs |
| **Normative TS** | `TZ_boa_sw_ServiceWorkers.md` §2 (all), §3.2 (`AD-1…AD-12` for the ADR seed), §15.6, §16.0, §1.4 (for `docs/compat.md`), §17.3 (`D-1…D-5`) |
| **Prerequisites** | none — this is the first work order |
| **Diff budget** | ~700 lines (stop and escalate above ~1050) |
| **Architect's role** | Every file's content is specified below, most of it verbatim. |
| **Implementer's role** | Create the files exactly as written, make the build green, write the handoff. **No code beyond stubs. No model, no algorithms, no bindings.** |

---

## 1. Guardrails

Read these before touching anything; they are the rules this task establishes for every later task.

1. **One work order at a time.** This task creates scaffolding only. If you find yourself writing a
   `struct` with fields or a function with a body, you have left the task.
2. **`R2.2.1` — dependency direction is one-way:**
   `boa_sw_core` ← `boa_sw_fetch` ← `boa_sw`; `boa_sw_dom` ← `boa_sw`;
   `boa_sw_memory` / `boa_sw_sqlite` depend only on `boa_sw_core`; `boa_sw_wpt` depends on everything.
3. **`R2.2.2` — `boa_sw_core` MUST NOT depend on `boa_engine`, `boa_gc`, `boa_runtime` or
   `boa_wintertc`**, not even in `[dev-dependencies]`. This is checked mechanically (§6, criterion 4).
4. **`R2.1.2` — no `unwrap`/`expect`/`panic!` in library code**; **`R1.1.5` — no `unsafe` anywhere.**
   Both are enforced by workspace lints created in this task.
5. **`R2.3.1` — no dependency outside TS §2.3** without an ADR paragraph in `docs/DECISIONS.md`
   written *before* first use.
6. **`D-1` (TS §17.3): SQLite is the default backend.** `sqlite-backend` belongs to
   `default-features` of `boa_sw`. Do not "simplify" it out because it slows the build.
7. Versions are pinned by the workspace. A crate's `Cargo.toml` uses `x = { workspace = true }`,
   never a literal version, except for the `boa_*` crates where the literal `~0.22.0` is written in
   `[workspace.dependencies]` only.

---

## 2. Files to create

```
Cargo.toml                                   # workspace manifest
rust-toolchain.toml
clippy.toml
deny.toml
.gitignore
crates/boa_sw_core/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw_dom/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw_fetch/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw_memory/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw_sqlite/{Cargo.toml,README.md,src/lib.rs}
crates/boa_sw_wpt/{Cargo.toml,README.md,src/lib.rs,src/main.rs}
docs/SPEC_REVISION.md
docs/DECISIONS.md
docs/QUESTIONS.md
docs/compat.md
docs/traceability.md
docs/reviews/.gitkeep
.github/workflows/ci.yml
```

Already present, do **not** modify: `TZ_boa_sw_ServiceWorkers.md`, `AGENTS.md`, `README.md`,
`docs/BOA_022_PLATFORM_NOTES.md`, `tasks/TASK_TEMPLATE.md`.

---

## 3. Exact file contents

### 3.1. `Cargo.toml` (workspace root)

```toml
[workspace]
resolver = "3"
members = [
    "crates/boa_sw_core",
    "crates/boa_sw_dom",
    "crates/boa_sw_fetch",
    "crates/boa_sw",
    "crates/boa_sw_memory",
    "crates/boa_sw_sqlite",
    "crates/boa_sw_wpt",
]
# `examples/offline_shell` joins the workspace in T-23; `fuzz/` has its own workspace (T-25).

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.91.0"
license = "MIT OR Apache-2.0"
authors = ["BoaX Developers"]
repository = "https://github.com/boa-dev/boa"

[workspace.dependencies]
# --- internal ---
boa_sw_core = { version = "0.1.0", path = "crates/boa_sw_core" }
# Note: do NOT add `default-features = false` here — dependents inherit it and would silently
# lose the `std` feature of the core crate.
boa_sw_dom = { version = "0.1.0", path = "crates/boa_sw_dom" }
boa_sw_fetch = { version = "0.1.0", path = "crates/boa_sw_fetch" }
boa_sw = { version = "0.1.0", path = "crates/boa_sw" }
boa_sw_memory = { version = "0.1.0", path = "crates/boa_sw_memory" }
boa_sw_sqlite = { version = "0.1.0", path = "crates/boa_sw_sqlite" }

# --- engine (TS §2.3) ---
boa_engine = "~0.22.0"
boa_gc = "~0.22.0"
boa_macros = "~0.22.0"
boa_runtime = "~0.22.0"
boa_wintertc = "~0.22.0"

# --- shared ---
thiserror = "2.0"
url = "2.5"
http = "1"
indexmap = "2.7"
smallvec = { version = "1.13", features = ["union", "const_generics"] }
hashbrown = "0.15"
bitflags = "2"
sha2 = "0.10"
tracing = "0.1"
serde = { version = "1", features = ["derive"] }
futures-lite = "2"
futures-channel = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }

# --- dev ---
proptest = "1.6"
rstest = "0.24"
insta = "1"

[workspace.lints.rust]
unsafe_code = "deny"
missing_docs = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
must_use_candidate = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
module_name_repetitions = "allow"
unwrap_used = "warn"
expect_used = "warn"
```

> If a `boa_*` crate at `~0.22.0` cannot be resolved from crates.io in your environment, do **not**
> change the version. Add, as the last lines of the root manifest, a commented-out block and record
> the situation in `docs/QUESTIONS.md`:
> ```toml
> # [patch.crates-io]
> # boa_engine = { path = "../boa/core/engine" }
> # boa_gc = { path = "../boa/core/gc" }
> ```

### 3.2. `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.91.0"
components = ["rustfmt", "clippy"]
targets = ["wasm32-unknown-unknown"]
```

### 3.3. `clippy.toml`

```toml
avoid-breaking-exported-api = false
```

### 3.4. `deny.toml`

```toml
[licenses]
# Note: the `unlicensed` key was removed upstream (cargo-deny #611); unlicensed crates are
# rejected by default in current cargo-deny versions.
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unlicense",
    "Zlib",
    "CC0-1.0",
    # Required by the ICU crates pulled in through boa_engine.
    "Unicode-3.0",
    "Unicode-DFS-2016",
]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
deny = []

[sources]
unknown-registry = "warn"
unknown-git = "warn"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

### 3.5. `.gitignore`

```
/target
**/*.rs.bk
Cargo.lock.bak
.DS_Store
/wpt-results
```

`Cargo.lock` **is** committed (this workspace produces binaries and a reproducible CI).

### 3.6. Crate manifests

Each crate's `[package]` block uses workspace inheritance and its own `description`.
Every crate ends with:

```toml
[lints]
workspace = true
```

**`crates/boa_sw_core/Cargo.toml`**

```toml
[package]
name = "boa_sw_core"
description = "Engine-independent core model and algorithms for the Service Workers implementation"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
thiserror = { workspace = true }
url = { workspace = true }
http = { workspace = true }
indexmap = { workspace = true }
smallvec = { workspace = true }
hashbrown = { workspace = true }
bitflags = { workspace = true }
sha2 = { workspace = true }
tracing = { workspace = true, optional = true }
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true }
rstest = { workspace = true }
insta = { workspace = true }

[features]
default = ["std"]
std = []
tracing = ["dep:tracing"]
serde = ["dep:serde", "indexmap/serde", "url/serde"]
test-util = []

[lints]
workspace = true
```

**`crates/boa_sw_dom/Cargo.toml`**

```toml
[package]
name = "boa_sw_dom"
description = "Minimal DOM shim (EventTarget, Event, DOMException, MessageEvent, AbortController) for Boa"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
boa_engine = { workspace = true }
boa_gc = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
rstest = { workspace = true }

[features]
default = []

[lints]
workspace = true
```

**`crates/boa_sw_fetch/Cargo.toml`**

```toml
[package]
name = "boa_sw_fetch"
description = "Fetch API subset (Headers, Request, Response, fetch) required by Service Workers"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
boa_sw_core = { workspace = true }
boa_engine = { workspace = true }
boa_gc = { workspace = true }
boa_runtime = { workspace = true, optional = true }
http = { workspace = true }
url = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
rstest = { workspace = true }
proptest = { workspace = true }

[features]
default = []
runtime-interop = ["dep:boa_runtime"]

[lints]
workspace = true
```

**`crates/boa_sw/Cargo.toml`**

```toml
[package]
name = "boa_sw"
description = "Service Workers implementation for the Boa JavaScript engine"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
boa_sw_core = { workspace = true }
boa_sw_dom = { workspace = true, optional = true }
boa_sw_fetch = { workspace = true, optional = true }
boa_sw_memory = { workspace = true, optional = true }
boa_sw_sqlite = { workspace = true, optional = true }
boa_engine = { workspace = true }
boa_gc = { workspace = true }
boa_runtime = { workspace = true, optional = true }
boa_wintertc = { workspace = true, optional = true }
thiserror = { workspace = true }
url = { workspace = true }
http = { workspace = true }
indexmap = { workspace = true }
futures-lite = { workspace = true }
futures-channel = { workspace = true }
tracing = { workspace = true, optional = true }

[dev-dependencies]
boa_sw_memory = { workspace = true }
rstest = { workspace = true }
insta = { workspace = true }

[features]
default = ["dom-shim", "fetch-classes", "memory-backend", "sqlite-backend", "module-workers", "navigation-preload"]
dom-shim = ["dep:boa_sw_dom"]
fetch-classes = ["dep:boa_sw_fetch"]
memory-backend = ["dep:boa_sw_memory"]
sqlite-backend = ["dep:boa_sw_sqlite"]
module-workers = []
navigation-preload = []
runtime-interop = ["dep:boa_runtime", "dep:boa_wintertc", "boa_sw_fetch?/runtime-interop"]
tracing = ["dep:tracing", "boa_sw_core/tracing"]

[lints]
workspace = true
```

**`crates/boa_sw_memory/Cargo.toml`**

```toml
[package]
name = "boa_sw_memory"
description = "In-memory storage backend for boa_sw (registrations, script maps, cache storage)"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
boa_sw_core = { workspace = true }
indexmap = { workspace = true }
hashbrown = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
boa_sw_core = { workspace = true, features = ["test-util"] }
rstest = { workspace = true }

[features]
default = []

[lints]
workspace = true
```

**`crates/boa_sw_sqlite/Cargo.toml`**

```toml
[package]
name = "boa_sw_sqlite"
description = "SQLite storage backend for boa_sw — the default, persistent backend"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[dependencies]
boa_sw_core = { workspace = true }
rusqlite = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
boa_sw_core = { workspace = true, features = ["test-util"] }
rstest = { workspace = true }

[features]
default = []

[lints]
workspace = true
```

**`crates/boa_sw_wpt/Cargo.toml`**

```toml
[package]
name = "boa_sw_wpt"
description = "web-platform-tests harness for boa_sw (development tool, not published)"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
publish = false

[dependencies]
boa_sw = { workspace = true }
boa_sw_core = { workspace = true }
boa_engine = { workspace = true }

[features]
default = []

[lints]
workspace = true
```

### 3.7. Crate `src/lib.rs` stubs

Every stub follows this shape; only the doc text changes. No other items.

```rust
//! <ROLE PARAGRAPH — see the table below>
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in <TASK>.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
```

**Convention established here (applies to every later task):** the workspace denies
`clippy::unwrap_used` / `clippy::expect_used`, and CI runs clippy with `--all-targets`, which
includes tests. Unit tests are exempted by the `cfg_attr` line above. Integration tests are separate
crates and do not inherit it, so every file under `crates/*/tests/` starts with:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
```

| Crate | Role paragraph (first line of the doc comment) | Status line task |
|---|---|---|
| `boa_sw_core` | `Engine-independent core of the Service Workers implementation: model, job queue, lifecycle algorithms, fetch decision logic, cache algorithms and storage traits. Contains no dependency on a JavaScript engine (TS AD-2, R2.2.2).` | `T-02` |
| `boa_sw_dom` | `Minimal DOM shim required by Service Workers: EventTarget, Event, DOMException, MessageEvent, ErrorEvent and AbortController for the Boa engine (TS §5.5).` | `T-11` |
| `boa_sw_fetch` | `The Fetch API subset Service Workers need: Headers, Request, Response, the Body mixin and fetch(), with request mode/credentials/destination and response type/redirected modelled (TS AD-8).` | `T-12` |
| `boa_sw` | `Service Workers for the Boa JavaScript engine: the runtime (realms, scheduler, host API) and every JavaScript-visible class (TS §4, §5, §11).` | `T-13` |
| `boa_sw_memory` | `In-memory implementation of the boa_sw_core storage traits. Intended for tests and for hosts that do not need persistence; the default backend is boa_sw_sqlite (TS D-1).` | `T-08` |
| `boa_sw_sqlite` | `SQLite implementation of the boa_sw_core storage traits — the default, persistent backend for registrations, script resource maps and cache storage (TS D-1, Appendix C).` | `T-22` |
| `boa_sw_wpt` | `web-platform-tests harness for boa_sw: runs the service-workers test suite against a fake host and compares results with expectations.json (TS §15.3).` | `T-24` |

`crates/boa_sw_wpt/src/main.rs`:

```rust
//! Entry point of the WPT harness. Implemented in `T-24`.

fn main() {
    eprintln!("boa_sw_wpt: not implemented yet (see work order T-24)");
    std::process::exit(2);
}
```

### 3.8. Crate `README.md` files

One per crate, this exact shape (substitute the crate name, the role paragraph from §3.7 and the
pipeline position):

```markdown
# <crate name>

<role paragraph>

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: <one sentence saying where it sits and what it may depend on.>**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: <list them>.

Status: skeleton; implementation starts in work order `<T-NN>`.
```

### 3.9. `docs/SPEC_REVISION.md`

```markdown
# Pinned specification revisions

All requirements in `TZ_boa_sw_ServiceWorkers.md` are stated against these revisions.
A newer editor's draft is **not** authoritative until this file is updated by a change request.

| Specification | URL | Revision used | Retrieved |
|---|---|---|---|
| Service Workers | https://www.w3.org/TR/service-workers/ | <"Latest published version" date shown on the page> | <YYYY-MM-DD> |
| Service Workers (editor's draft) | https://w3c.github.io/ServiceWorker/ | commit `<sha>` | <YYYY-MM-DD> |
| Fetch | https://fetch.spec.whatwg.org/ | commit `<sha>` | <YYYY-MM-DD> |
| DOM | https://dom.spec.whatwg.org/ | commit `<sha>` | <YYYY-MM-DD> |
| HTML (workers, messaging, structured clone) | https://html.spec.whatwg.org/ | commit `<sha>` | <YYYY-MM-DD> |
| URL | https://url.spec.whatwg.org/ | commit `<sha>` | <YYYY-MM-DD> |

## How to fill this in

Each WHATWG spec footer shows the current commit SHA; the W3C page shows the publication date.
Record what you actually read. If you cannot reach a specification, write `unavailable` plus the
date and raise it in `docs/QUESTIONS.md` — do not guess a SHA.
```

The implementer fills the placeholders with what they actually retrieved. If network access is not
available in the environment, every cell gets `unavailable (<date>)` and one entry is added to
`docs/QUESTIONS.md`; this does **not** block the task.

### 3.10. `docs/DECISIONS.md`

Seed with the twelve architectural decisions plus the five customer decisions:

```markdown
# Decision log (ADR)

Format: one section per decision. Architectural decisions `AD-*` and customer decisions `D-*` are
copied from `TZ_boa_sw_ServiceWorkers.md` (§3.2, §17.3) and are **not** open for revision by an
implementer — see `AGENTS.md`. Decisions taken *during* implementation (including every new
dependency, per `R2.3.1`) are appended below them with the id `I-<NN>`.

## AD-1 — Realm per service worker, one Context, one thread
<one paragraph: the decision, then the rationale, copied from TS §3.2>

## AD-2 — Engine-free core
...
## AD-12 — Explicit error taxonomy
...

## D-1 — SQLite is the default storage backend
<decision + rationale from TS §17.3>
...
## D-5 — The host owns ClientId lifecycle
...

## I-01 — <first implementation decision>
(none yet)
```

Every `AD-*` and `D-*` section is one paragraph. Do not paraphrase loosely — copy the wording from
the TS so the two documents cannot drift.

### 3.11. `docs/QUESTIONS.md`

```markdown
# Escalations

Anything an implementer must not decide alone goes here (see `AGENTS.md`, "Escalate").
One section per question, newest last. Do not delete answered questions; mark them **Answered**.

Template:

## Q-<NN> — <short title>
- **Raised by / task:** <name> / `T-NN`
- **Blocking?** yes | no (what is blocked)
- **Context:** what you were doing, which requirement id is involved
- **Options considered:** at least two, with consequences
- **Recommendation:** yours
- **Answer:** *(filled by the architect/customer)*
```

### 3.12. `docs/compat.md`

Pre-fill from TS §1.4 — one section per exclusion, each stating the **observable** behaviour:

```markdown
# Compatibility: deviations from the Service Workers specification

Every deviation is listed here with the behaviour a script actually observes. This file is part of
the acceptance criteria (TS §17.1 item 1) and grows only through approved decisions.

| # | Area | Deviation | Observable behaviour | Source |
|---|---|---|---|---|
| C-01 | Push, Notifications, Background/Periodic Sync, Background Fetch, Payment Handler, Cookie Store | Not implemented | The corresponding interfaces are absent from the global scope; `self.registration.pushManager` is `undefined` | TS §1.4 |
| C-02 | Streams | `Response.body` is always `null`; bodies are fully buffered | `respondWith` of a streamed response is impossible; a body above `max_body_bytes` (32 MiB default) becomes a network error, and `new Response(x)` with an oversized body throws `RangeError` | TS §1.4, AD-7 |
| C-03 | `Blob` / `File` / `FormData` | No File API in Boa | `Response.blob()` rejects with `NotSupportedError`; `formData()` supports only `application/x-www-form-urlencoded` | TS §1.4 |
| C-04 | CSP, Trusted Types, COOP/COEP | Not enforced by this crate | The host may reject script fetches through `HttpClient`; no exception is raised by `boa_sw` itself | TS §1.4 |
| C-05 | Multi-process access to one storage key | Rejected | Opening a SQLite database already held by another process fails with an error at `install` time | TS §1.4, R13.6 |
| C-06 | Preemptive termination | Cooperative only | A worker looping forever cannot be killed; timeouts take effect between jobs | TS §1.4, AD-10 |
| C-07 | `SharedWorker` clients | Never created by this crate | `Client.type` may be `"sharedworker"` only if the host reports such a client | TS §1.4 |
| C-08 | Cross-origin `importScripts` with opaque responses | Rejected | The update job fails with `SecurityError` | TS §1.4 |
| C-09 | HTTP cache | Not implemented here | `updateViaCache` is translated into a `CachePolicy` handed to the host `HttpClient`, which decides | TS §1.4, R6.4.4 |
| C-10 | `importScripts` fetch model | Collect-then-evaluate with a bounded retry | Side effects before a previously unseen `importScripts` URL may run twice during the first evaluation of a worker | TS R11.4.4 |
| C-11 | `Cache` object of a deleted cache | Rejects instead of operating on a detached snapshot | Operations on a `Cache` whose cache was deleted reject with `InvalidStateError` | TS R9.3.8 |
```

### 3.13. `docs/traceability.md`

```markdown
# Traceability

Two tables, both required by TS §15.2.2 and checked at release by `scripts/check_traceability.rs`
(written in `T-26`).

## 1. Requirements → tests

| Requirement | Where implemented | Covering tests | Task |
|---|---|---|---|
| `R2.1.1` | workspace lints, CI | `ci: fmt`, `ci: clippy`, `ci: deny` | T-01 |
| `R2.2.1` | crate manifests | `ci: dependency-direction` | T-01 |
| `R2.2.2` | `crates/boa_sw_core/Cargo.toml` | `ci: dependency-direction` | T-01 |
| `R2.4.1` | crate features | `ci: feature matrix` | T-01 |

*(every later task appends its rows)*

## 2. Specification algorithms → modules

Copy of TS Appendix E, extended with a "covering tests" column as tasks land.
```

### 3.14. `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.91.0
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.91.0
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        features:
          - "--all-features"
          - ""                                   # default features (includes sqlite, per D-1)
          - "--no-default-features"
          - "--no-default-features --features memory-backend"
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.91.0
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace ${{ matrix.features }}

  wasm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.91.0
        with:
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
      - run: cargo build -p boa_sw_core --target wasm32-unknown-unknown
      - run: cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features

  dependency-direction:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.91.0
      - name: boa_sw_core must not depend on the engine (R2.2.2)
        run: |
          set -euo pipefail
          if cargo tree -p boa_sw_core --all-features --edges normal,build,dev \
             | grep -E '(^|[^a-z_])boa_(engine|gc|runtime|wintertc|macros)' ; then
            echo "::error::boa_sw_core depends on the engine — violates R2.2.2"
            exit 1
          fi

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2

  coverage:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - run: echo "coverage gate not implemented yet (T-26 wires it; thresholds in TS R15.2.1)"

  wpt:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - run: echo "WPT gate not implemented yet (T-24 wires it; contract in TS R15.3.3)"
```

---

## 4. Procedure

1. Create every file of §2 with the content of §3.
2. `cargo generate-lockfile`, then `cargo build --workspace --all-features`. Fix only manifest
   problems; if a listed dependency version does not exist, **do not invent a different crate** —
   record it in `docs/QUESTIONS.md` and use the nearest existing patch release of the same minor
   version.
3. Run every command of §6 locally until all are green.
4. Fill `docs/SPEC_REVISION.md` with what you actually retrieved.
5. Write `docs/reviews/T-01-handoff.md` per §8.

---

## 5. Tests to write

None. This task has no library code, therefore no unit tests. Its verification is entirely the
command list in §6, which CI reproduces. Do **not** add placeholder `#[test] fn it_works()` stubs.

---

## 6. Acceptance criteria

Each criterion is proven by the exact command shown; record the output summary in the handoff.

| # | Criterion | Command |
|---|---|---|
| 1 | Workspace builds with all features | `cargo build --workspace --all-features` |
| 2 | Workspace builds with no default features | `cargo build --workspace --no-default-features` |
| 3 | Workspace builds with default features (must include the SQLite backend, `D-1`) | `cargo build --workspace` then `cargo tree -p boa_sw -e normal \| grep -q boa_sw_sqlite` |
| 4 | `boa_sw_core` is engine-free (`R2.2.2`) — **no output expected** | `cargo tree -p boa_sw_core --all-features --edges normal,build,dev \| grep -E 'boa_(engine\|gc\|runtime\|wintertc\|macros)'` |
| 5 | `boa_sw_core` builds for wasm, both feature sets | `cargo build -p boa_sw_core --target wasm32-unknown-unknown` and the same with `--no-default-features` |
| 6 | Formatting | `cargo fmt --all --check` |
| 7 | Lints, including the pedantic set | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| 8 | Licences and sources | `cargo deny check` (needs network for the advisory DB; offline, run `cargo deny check licenses bans sources` and say so in the handoff) |
| 9 | Test runner works (zero tests is the expected result) | `cargo test --workspace --all-features` |
| 10 | Dependency direction (`R2.2.1`) holds for every crate | `cargo tree -p boa_sw_dom -e normal \| grep -q boa_sw_core` returns **non-zero** (dom must not depend on core); `cargo tree -p boa_sw_memory -e normal \| grep -q boa_engine` returns **non-zero** |
| 11 | All files of §2 exist | `ls` / `git status --porcelain` shows exactly those additions and nothing else |
| 12 | `docs/DECISIONS.md` contains 12 `AD-*` sections and 5 `D-*` sections | `grep -c '^## AD-' docs/DECISIONS.md` → `12`; `grep -c '^## D-' docs/DECISIONS.md` → `5` |
| 13 | `docs/compat.md` contains the 11 pre-filled deviations | `grep -c '^| C-' docs/compat.md` → `11` |
| 14 | Every crate README links `AGENTS.md` | `grep -L 'AGENTS.md' crates/*/README.md` prints nothing |
| 15 | The WPT binary exits with the "not implemented" contract | `cargo run -p boa_sw_wpt`; expect exit code 2 and the message on stderr |
| 16 | CI workflow is valid YAML and defines the jobs of `R15.6.2` | `python3 -c "import yaml,sys; d=yaml.safe_load(open('.github/workflows/ci.yml')); print(sorted(d['jobs']))"` → contains `clippy`, `coverage`, `dependency-direction`, `deny`, `fmt`, `test`, `wasm`, `wpt` |

---

## 7. Do not do

- No model types, no traits, no algorithms, no JavaScript bindings — not even "obvious" ones such as
  `SwError` (that is `T-02`) or the id newtypes.
- No extra modules beyond `lib.rs` (and `main.rs` for the harness).
- No dependencies beyond §3.1, and no version bumps "because a newer one exists".
- No `#[test]` placeholders, no `todo!()`/`unimplemented!()` in library code.
- Do not create `examples/offline_shell` or `fuzz/` (T-23 and T-25 own them).
- Do not edit `TZ_boa_sw_ServiceWorkers.md`, `AGENTS.md`, the root `README.md` or
  `docs/BOA_022_PLATFORM_NOTES.md`. If you believe the TS is wrong, use `docs/QUESTIONS.md`.

---

## 8. Handoff

Write `docs/reviews/T-01-handoff.md` containing:

1. The file tree that was created.
2. All 16 acceptance-criteria commands with their output summary (exit codes; for criterion 4 an
   explicit "no output").
3. Deviations — expected: none. Anything else must also appear in `docs/QUESTIONS.md`.
4. `docs/DECISIONS.md` entries added (`AD-1…AD-12`, `D-1…D-5`, plus any `I-*` you were forced to
   make, e.g. a patched dependency version).
5. Which specification revisions you managed to pin, and which are `unavailable`.
6. Requirement ids covered (`R2.1.1`, `R2.2.1`, `R2.2.2`, `R2.4.1`, `R2.4.2` partly) mirrored into
   `docs/traceability.md`.
7. Open questions for `T-02`.

Then stop. `T-02` (error taxonomy, ids, storage key, clock, observer) is the next work order.
