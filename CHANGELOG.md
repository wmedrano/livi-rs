# Changelog

## Unreleased

### Fixed

- Use padded size to check `LV2AtomSequence::push`. Previously, we may have
  overestimated the capacity.
- Reject CV input and output buffers shorter than the requested frame count before
  running the plugin, returning `CVInputSampleCountTooSmall` or
  `CVOutputSampleCountTooSmall` as appropriate.
