# Plugin safety audit

This review covers `Plugin::instantiate`, `Instance::run`, their port connections,
worker callbacks, and the state and destruction paths they enable. It is a focused
contract review, not a proof that livi or arbitrary plugins are memory safe.

## Caller guarantees

- Load native plugins whose metadata matches their implementation and whose code
  follows the [LV2 core contracts](https://lv2plug.in/ns/lv2core). The guarantee
  applies throughout the instance lifetime, including safe state methods and Drop.
- Supply a finite, positive sample rate supported by the plugin.
- Supply buffers in port-index order within each type, using iterators whose
  reported lengths equal their actual remaining element counts.
- Use the instance's URID mapping for atom buffers and payloads, and satisfy the
  plugin's event protocols. Native output must remain within capacity and contain
  valid headers, event sizes, and padding.
- Keep buffers valid throughout `run`. Plugins must not access borrowed port data
  after it returns. Raw calls must reconnect buffers before using expired port
  pointers and preserve livi's ownership, lifecycle, and threading invariants.
- Respect [LV2 Worker](https://lv2plug.in/ns/ext/worker) callback contexts and
  concurrency. Worker execution can overlap audio processing; externally invoking
  the same instance must not race livi's operations.

## Wrapper guarantees and limits

The wrapper retains plugin and feature ownership, connects control storage, and
checks frame bounds, reported port counts, and audio/CV buffer lengths before
processing. An error can leave some ports reconnected or atom outputs cleared.
The next successful livi `run` reconnects all non-control ports.

Rust references enforce ordinary buffer validity and output exclusivity. They do
not validate plugin machine code, atom payloads, or custom iterator length claims.
Output iteration trusts native sequence headers; malformed output is not
guaranteed to panic safely. The public raw-pointer scheduling callback is now
unsafe and documents its handle, payload, and serialization requirements.

## Library follow-up before 1.0

`LV2AtomSequence` stores its buffer in `Vec<u8>`, then casts it to LV2 atom
structures. A byte allocation only guarantees byte alignment, while the
[LV2 Atom specification](https://lv2plug.in/ns/ext/atom) requires 8-byte alignment.
The library must use storage with a guaranteed suitable alignment. Typical
allocator behavior and passing tests do not establish this guarantee. This is a
library responsibility, not an additional caller precondition, and remains open.
