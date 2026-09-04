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
