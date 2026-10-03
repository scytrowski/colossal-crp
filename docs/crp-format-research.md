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

Additional fields and text-readable names or identifiers follow the package name, but their boundaries and meanings have not yet been confirmed. Strings in the sample include `Wolf Creek_SteamPreview`, `Wolf Creek_Snapshot`, and `Wolf Creek_Data`; .NET type names and PNG data also appear later. These strings should not yet be treated as a complete entry listing, or as proof that every file contains the same set of entries.

## Findings from the DLLs

1. The format is a package container rather than a single raw map image: the libraries include dedicated package readers and writers, an index table, and a collection of entries.
2. The container has a versioned header. The supplied corpus contains multiple versions, so a Rust reader should preserve and explicitly validate the version number.
3. The package code includes the concept of compressed data (`isCompressed`, `Compress`, `Uncompress`). The flag layout, algorithm, compression scope, and length encoding still need confirmation from IL or by correlating multiple samples.
4. The class names suggest a separation between container handling (reader, writer, index, entries) and game-object serialization (`PackageSerializer`, `DataSerializer`). It is useful to investigate these layers separately: first extract entries, then decode map resources.

## Open questions

- The complete order and types of header fields after the package name.
- The index entry structure: name or path, type, offset, compressed and uncompressed lengths, flags, and any checksum.
- The location of the index table (beginning, end, or elsewhere) and whether there can be multiple tables.
- Compression details and payload type serialization.
- Semantic differences between versions 4–10.

## Next research steps

1. Inspect the IL for `PackageReader`, `PackageWriter`, and `ReadIndexTable` in `ColossalManaged.dll`, including class fields and method signatures.
2. Trace related call paths in `Assembly-CSharp.dll` to distinguish container writing from map serialization.
3. Decode headers and indexes from files of several versions; validate offsets by checking payload signatures (such as PNG) and file boundaries.
4. Build a compatibility table for versions 4–10 before defining the public Rust API.

## Limitations of this pass

The environment provided the DLLs and tools for reading strings from binaries, but no local .NET decompiler (`monodis`, `ilspycmd`, or `ildasm`) was found. CLI metadata inspection can identify classes and methods, but it does not replace IL analysis. The file-prefix observations above were checked directly against samples; the remaining structural claims are hypotheses for further verification.
