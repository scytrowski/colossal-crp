# Agent instructions

## Project goal

Build a Rust library for reading and writing City: Skylines (2015) `.crp` files. Reading and writing are equally important project goals. Keep format research, implementation, and public API decisions aligned with both directions.

## Language and documentation

- Write repository documentation, code comments, and user-facing examples in English.
- Keep reverse-engineering findings in `docs/crp-format-research.md`; update it when implementation work establishes or changes a format fact.
- Distinguish confirmed behavior from hypotheses. For confirmed findings, record the DLL or method examined and the sample scope used to validate it. Do not present a corpus observation as a guarantee for every `.crp` file.

## Format research

- Treat `data/dlls/` and `data/maps/` as read-only research inputs. Do not modify or replace them.
- Derive behavior from the game/framework DLLs and verify it against available map samples where possible.
- Keep outer package versions, inner serializer versions, endianness, and version gates explicit. Do not assume that similarly named version fields are interchangeable.
- Do not infer package-wide behavior from one asset type. The `.crp` container, asset payloads, and game-specific serialized objects are separate format layers.
- When a field or asset type is not understood, preserve its raw bytes or represent it as unknown when practical. Do not silently discard data during a read/write round trip.

## Reader and writer design

- Support both reading and writing. Prefer shared format models and paired read/write logic so the two paths agree on field order, versions, offsets, lengths, and checksums.
- Make version-dependent behavior explicit. Reject unsupported or malformed input with useful errors rather than panicking or reading beyond declared bounds.
- Preserve information needed for faithful rewriting, including unknown asset types and payloads, entry ordering where relevant, and values not yet modeled.
- Do not normalize or rewrite payloads implicitly. Any lossy conversion or canonicalization must be explicit in the API and documented.
- Keep checksum calculation over the exact payload bytes required by the format; do not hash decoded or re-encoded content unless the format specifies that behavior.
- Avoid claiming full round-trip support for a file or payload family until read/write behavior is understood for that family and version.

## Rust implementation

- Keep the public API small and idiomatic. Separate container/index handling from asset-specific payload decoding.
- Prefer checked arithmetic and bounded reads for offsets, lengths, and counts originating in files.
- Keep dependencies minimal; explain the need before adding a dependency for core parsing or serialization.
- Follow the crate's existing style and edition (`2024`). Do not add unrelated features or reorganize files without a project need.

## Changes and reporting

- Keep changes focused and update relevant English documentation alongside code when format behavior changes.
- Describe what is implemented, which versions and payload families it covers, and any remaining limitations. State clearly when a behavior is inferred rather than confirmed.
- Use concise commit messages that describe the change when commits are requested.
