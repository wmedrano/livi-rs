# Changelog

## Unreleased

## 0.9.0

### Breaking changes

- Mark the raw-pointer worker callback `features::worker::schedule_work` as
  unsafe to call directly.
- Mark all public error enums as `#[non_exhaustive]` so future variants can be added
  without breaking exhaustive matches. Downstream matches must include a wildcard
  arm.
- Add `RunError::CVInputSampleCountTooSmall` and
  `RunError::CVOutputSampleCountTooSmall`. Downstream exhaustive matches on
  `RunError` must handle these variants.
- Validate feature block bounds before creating features. `FeaturesBuilder::build`
  and `World::build_features` now panic if the minimum exceeds the maximum or
  either bound exceeds `i32::MAX`.
- Apply plugin validation to `World::with_load_bundle`, excluding plugins with
  unsupported required features or ports. Atom ports must explicitly declare
  `atom:Sequence` buffer support for all plugin loading methods.

### Added

- Document plugin instantiation and execution safety contracts, raw port-buffer
  lifetimes, and worker callback preconditions. Record audit follow-ups in
  `SAFETY.md`.
- Add `FeaturesBuilder::try_build`, `World::try_build_features`, and `FeatureError`
  for reporting invalid block bounds without panicking.

### Deprecated

- Deprecate `FeaturesBuilder::build` and `World::build_features` in favor of
  `try_build` and `try_build_features`, which return errors for invalid bounds.

### Fixed

- Safely handle worker payloads with an empty body, including null data pointers.
- Preserve complete worker messages across ring-buffer wraparound.
- Fix a worker race condition by committing header and body bytes in a single
  transaction.
- Use padded size to check `LV2AtomSequence::push_event`. Previously, we may have
  overestimated the capacity.
- Give `LV2AtomEventBuilder` a defined C field layout while retaining packing.
- Reject CV input and output buffers shorter than the requested frame count before
  running the plugin, returning `CVInputSampleCountTooSmall` or
  `CVOutputSampleCountTooSmall` as appropriate.
