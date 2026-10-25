# Changelog

## Unreleased

### Fixed

- Reject CV input and output buffers shorter than the requested frame count before
  running the plugin, returning `CVInputSampleCountTooSmall` or
  `CVOutputSampleCountTooSmall` as appropriate.
