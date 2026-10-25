# Changelog

## Unreleased

### Fixed

- Filter plugins with unsupported atom ports. Only sequence ports are supported.
- Allows plugins to safetly handle worker payloads with empty body.
- Fix worker race condition by committing header and body bytes in single
  transaction.
- Use padded size to check `LV2AtomSequence::push`. Previously, we may have
  overestimated the capacity.
- Reject CV input and output buffers shorter than the requested frame count before
  running the plugin, returning `CVInputSampleCountTooSmall` or
  `CVOutputSampleCountTooSmall` as appropriate.
