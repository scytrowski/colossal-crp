# Preliminary research on the `.crp` format

Status as of 2026-10-03. These notes are based on the game DLLs and samples in `data/maps/`. They record current observations, not a complete implementation specification.

## Materials examined

- `data/maps/`: 105 map files.
- All 105 files begin with the ASCII bytes `CRAP`.
- The little-endian `u16` at offset `0x04` has values from 4 through 10 in this sample corpus:

| Version | Map count |
| ---: | ---: |
| 4 | 21 |
| 5 | 2 |
| 6 | 20 |
| 7 | 4 |
| 8 | 16 |
| 9 | 16 |
| 10 | 26 |

- The main libraries relevant to the format are `data/dlls/ColossalManaged.dll` and `data/dlls/Assembly-CSharp.dll`.
- `data/dlls/ColossalManaged.dll` contains the names `PackageReader`, `PackageWriter`, `ReadIndexTable`, `m_IndexTable`, `m_Entries`, `m_PackageVersion`, `Compress`, and `Uncompress`, as well as package-related diagnostic strings.
- `data/dlls/Assembly-CSharp.dll` contains `PackageReader`, `PackageWriter`, `PackageSerializer`, `PackageDeserializer`, `DataSerializer`, `PackageHelper`, and `PackageManager`. This indicates that the game uses package APIs from the Colossal framework.
- `ICSharpCode.SharpZipLib.dll` is present in the DLL directory. Its presence alone does not prove that every `.crp` payload uses this library or identify the compression algorithm.

CLI metadata inspection confirmed the method owners in `ColossalManaged.dll`: `ColossalFramework.Packaging.Package.ReadIndexTable`, and the classes `ColossalFramework.Packaging.PackageReader` and `PackageWriter`. The reader and writer have method sets for Unity types, including `ReadUnityType`, `ReadAsset`, `ReadTexture2D`, `ReadVector*Array`, and corresponding `Write*` methods. This narrows the analysis: `Package.ReadIndexTable` should explain the container, while `PackageReader` and `PackageWriter` should explain object serialization within entries.

## Confirmed file prefix

The sample `data/maps/CL Wolf Creek.crp` begins as follows:

```text
offset  size     observation
0x00    4        ASCII `CRAP`
0x04    2        little-endian u16 = 6
0x06    1+N      7-bit length prefix, followed by the name `CL Wolf Creek`
```

For this name, the length prefix at byte `0x06` is `0x0d` (13 bytes). This encoding matches .NET `BinaryWriter.Write(string)`. The same kind of prefix appears for map names of different lengths; for example, `AP Asanu Beach` starts with `0x0e`.

Additional fields and text-readable names or identifiers follow the package name. Their container layout is now confirmed from IL below. Strings in the sample include `Wolf Creek_SteamPreview`, `Wolf Creek_Snapshot`, and `Wolf Creek_Data`; .NET type names and PNG data also appear later. These strings should not yet be treated as a complete entry listing, or as proof that every file contains the same set of entries.

## Container index confirmed from IL

The IL for `ColossalFramework.Packaging.Package.ReadIndexTable` was decoded. It confirms this read order:

```text
char[4]  package ID (`CRAP`)
u16      package format version
string   package name
string   package author
u32      package version
string   main asset name
i32      asset count
i64      base data offset
```

Each index record is then read as:

```text
string   asset name
string   asset checksum
i32      asset type
i64      asset offset, relative to the package data area
i64      asset size
```

For each entry the reader computes its stream offset as the record's relative offset plus the base data offset plus `PackageImplementation.dataOffset`. The base `PackageImplementation` returns zero for `dataOffset`; a packaged implementation has a separate override. The read size is converted to `i32` when the in-memory asset is constructed, despite being stored as `i64` in the index.

The record's second string is passed to the `Asset` constructor as its checksum. The sample uses 32-character hexadecimal values; their specific checksum algorithm has not yet been verified.

This layout matches `CL Wolf Creek.crp` exactly. Its header values are version `6`, package name `CL Wolf Creek`, empty author, package version `0`, main asset `CL Wolf Creek`, four records, and base data offset `337` (`0x151`). The four records are:

| Asset name | Type value | Relative offset | Size |
| --- | ---: | ---: | ---: |
| `Wolf Creek_SteamPreview` | 3 | 0 | 286,048 |
| `Wolf Creek_Snapshot` | 3 | 286,048 | 102,218 |
| `Wolf Creek_Data` | 52 | 388,266 | 4,215,002 |
| `CL Wolf Creek` | 100 | 4,603,268 | 2,908 |

The index ends at offset `0x151`, matching the base data offset. The final entry ends exactly at the file size (4,606,513 bytes). Asset type numeric meanings and whether offsets are identical in other package implementations remain to be checked.

## Findings from the DLLs

1. The format is a package container rather than a single raw map image: the libraries include dedicated package readers and writers, an index table, and a collection of entries.
2. The container has a versioned header and an index layout confirmed above. The supplied corpus contains multiple versions, so a Rust reader should preserve and explicitly validate the version number.
3. The package code includes the concept of compressed data (`isCompressed`, `Compress`, `Uncompress`). The flag layout, algorithm, compression scope, and length encoding still need confirmation from IL or by correlating multiple samples.
4. The class names suggest a separation between container handling (reader, writer, index, entries) and game-object serialization (`PackageSerializer`, `DataSerializer`). It is useful to investigate these layers separately: first extract entries, then decode map resources.

## Open questions

- The meanings of asset type values, checksum algorithm, and any version-specific changes to index fields.
- Whether data offsets and entry sizes require additional handling for all package implementation types.
- Compression details and payload type serialization.
- Semantic differences between versions 4–10.

## Next research steps

1. Decode `PackageWriter` and compare its field and entry order to the reader implementation.
2. Map asset type values and inspect the serializer used by type `52` (the map data entry in this sample).
3. Decode headers and indexes from files of several versions; validate offsets by checking payload signatures (such as PNG) and file boundaries.
4. Inspect compression and payload serialization, then build a compatibility table for versions 4–10 before defining the public Rust API.

## Limitations of this pass

No high-level .NET decompiler (`monodis`, `ilspycmd`, or `ildasm`) was found, so the index method was decoded from its CIL body and metadata with a small inspection script rather than rendered as decompiled source. The listed field order and offset calculation are confirmed by the method body and were cross-checked against one sample. Other package versions and the writer path still need verification.
