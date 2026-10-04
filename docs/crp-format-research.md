# Preliminary research on the `.crp` format

Status as of 2026-10-04. These notes are based on the game DLLs and samples in `data/maps/`. They record current observations, not a complete implementation specification.

## Rust implementation status

The crate has an outer-container reader and compact writer. `Package::read_from` reads package versions 4–10 using the base implementation's zero data offset; `Package::read_from_with_data_offset` accepts an implementation-specific offset. The reader retains index order, checksum strings, unknown numeric asset types, and raw payload bytes, and rejects payloads whose MD5 does not match the index. `Package::write_compact_to` emits assets in index order with recalculated contiguous offsets and MD5 checksums. It preserves the stored author field without decrypting or encrypting it.

`MapMetaData::decode` and `encode` support the 25-field reflected type-100 `MapMetaData` schema, including mod entries and trailing bytes. Other reflected metadata runtime types remain raw. `TextureAsset::decode` and `encode` support type-3 image wrappers, including the outer-version-6 anisotropic-level gate. Image bytes remain opaque and are preserved. `SimulationMetaData::decode_from` and `encode` handle the first type-52 DataSerializer root and return its consumed length. `SimulationManagerData::decode` and `encode` handle the second root's scalar fields, fixed build-index history, manager reference array, and mod serializable-data storage; queued manager-specific object bodies remain opaque and are preserved.

The default `cargo test --offline` suite uses small committed fixtures cut from game maps, covering metadata from package versions 4 and 10, a texture from version 6, an isolated 193-byte SimulationMetaData stream, and a 3,074-byte SimulationManager root-field prefix from version 6. It includes MD5 vectors, synthetic round-trips for outer versions 4–10, malformed inputs, typed metadata and texture round-trips, both DataSerializer root codecs, and exact writer index bytes. An ignored test can be run with `CRP_CORPUS_DIR` to parse and checksum a full external corpus and round-trip every type-3 texture, type-100 metadata asset, and both type-52 roots while preserving the manager object graph. Full map files are not required by or included in the default test suite.

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

The record's second string is passed to the `Asset` constructor as its checksum. The sample uses 32-character lowercase hexadecimal MD5 values; the algorithm and corpus-wide match are confirmed below.

This layout matches `CL Wolf Creek.crp` exactly. Its header values are version `6`, package name `CL Wolf Creek`, empty author, package version `0`, main asset `CL Wolf Creek`, four records, and base data offset `337` (`0x151`). The four records are:

| Asset name | Type value | Relative offset | Size |
| --- | ---: | ---: | ---: |
| `Wolf Creek_SteamPreview` | 3 | 0 | 286,048 |
| `Wolf Creek_Snapshot` | 3 | 286,048 | 102,218 |
| `Wolf Creek_Data` | 52 | 388,266 | 4,215,002 |
| `CL Wolf Creek` | 100 | 4,603,268 | 2,908 |

The index ends at offset `0x151`, matching the base data offset. The final entry ends exactly at the file size (4,606,513 bytes).

## Writer corroboration and asset types

`Package.Save` in `ColossalManaged.dll` writes the same header and index fields in the same order as `ReadIndexTable`. It writes asset payloads after the index, copying from a disk path, in-memory byte array, or an existing asset stream. For a non-empty package author it encrypts the author string with `SimpleAES`; an empty author is written as-is. A reader should expect the author string to be empty/plaintext or encrypted, depending on the file.

Before writing the index, `CalculateDataStartOffset` computes the payload start from the stream's current position, adds eight bytes for the base-offset field, then adds each record's encoded name and checksum string lengths plus 4 bytes for type, 8 for relative offset, and 8 for size. In `Save`, record offsets are cumulative payload lengths. This agrees with the sample's index ending exactly where payload data begins.

The static initialization of `.AssetType` in `ColossalManaged.dll` defines these built-in numeric values. `Assembly-CSharp.dll` extends `User` with the listed metadata subtypes:

| Value | Asset type |
| ---: | --- |
| 0 | Unknown |
| 1 | Object |
| 2 | Material |
| 3 | Texture |
| 4 | StaticMesh |
| 50 | Text |
| 51 | Assembly |
| 52 | Data |
| 53 | Package |
| 80 | Locale |
| 81 | LocaleOverride |
| 100 | User / MapMetaData |
| 101 | User / SaveGameMetaData |
| 102 | User / SystemMapMetaData |
| 103 | User / CustomAssetMetaData |
| 104 | User / ColorCorrection |
| 105 | User / DistrictStyleMetaData |
| 106 | User / MapThemeMetaData |
| 107 | User / MapThemeMapMetaData |
| 108 | User / ScenarioMetaData |

The map corpus contains entry types 3, 52, 100, 102, 105, and 107. Across 105 files the occurrence counts are 237, 110, 104, 5, 12, and 1 respectively. Thus type 3 is used by preview/snapshot assets, type 52 by map data, and type 100 by map metadata in the inspected sample.

The decoded reader layout was applied to all 105 map files. Every index parsed, every index ended at its declared base data offset, and every asset range fit within its file. This confirms the shared container layout across the observed format versions 4–10; it does not yet prove that all other `.crp` package families use the same implementation or payload encoding.

## Reflected metadata payloads

The type `100` main asset in `CL Wolf Creek.crp` uses the framework's reflected object serializer. Its header is `bool isNull`, assembly-qualified runtime type string, object-name string, and little-endian `i32` field count. In this sample the values are `false`, `MapMetaData, Assembly-CSharp, ...`, `CL Wolf Creek`, and `25`.

Each reflected field begins with a null marker. A non-null field then stores its runtime type name and field name as .NET `BinaryWriter` strings, followed by its value. Null fields store only the marker; their identity comes from the reflected field order. Arrays store a little-endian `i32` length and then each item. DateTime values are round-trip strings (`DateTime.ToString("O")`). `PackageWriter.WriteUnityType` handles supported Unity/framework types and `PackageDeserializer.DeserializeObject` reads the corresponding field records.

The 25 `MapMetaData` fields in `CL Wolf Creek` were parsed to the end of the 2,908-byte entry. Values include `builtin=true`, `environment="North"`, `mapThemeRef=null`, `mapName="Wolf Creek"`, timestamp `2019-01-30T13:02:49.7692598+02:00`, image and preview checksums `1419a823a786f548fac710b6bde981b5` and `f52b6e1764cf54d1ed79295494e8ed0f`, `mods=[]`, and the inherited `assetRef` checksum `3bb3661184961300c39107046eea3e0d` (the `_Data` entry). Remaining fields are the published hash, buildable area, resource quantities, and route counts.

The implemented type-100 codec handles this field order: `builtin` (Boolean), `environment` (String), `mapThemeRef` (asset reference), `mapName` (String), `timeStamp` (DateTime round-trip string), `imageRef` and `steamPreviewRef` (asset references), `publishedHash` and `buildableArea` (`i32`), `maxResourceAmount` (`f32`), five resource amounts (`u32`), eight route counts (`i32`), `mods` (`ModInfo[]`), and inherited `assetRef` (asset reference). An asset reference's serialized value is its checksum string. `mods` is an `i32` count followed by each mod name as a BinaryWriter string and its published file ID as `i64`. Null fields retain their position in the 25-field layout and store only the null marker. Some version-4 metadata payloads have zero padding after the final field; the codec preserves trailing bytes so decode/encode reproduces those payloads exactly.

Across the corpus, the reflected metadata headers group as follows:

| Outer type | Count | Runtime type | Top-level field count |
| ---: | ---: | --- | ---: |
| 100 | 104 | `MapMetaData` | 25 |
| 102 | 5 | `SystemMapMetaData` | 4 |
| 105 | 12 | `DistrictStyleMetaData` | 8 |
| 107 | 1 | `SystemMapMetaData` | 4 |

The type-107 sample is an observed mismatch: the DLL's `UserAssetType` constants label 107 as `MapThemeMapMetaData`, but the entry payload advertises `SystemMapMetaData`. Preserve the outer type and inner runtime type independently when reading.

Type-3 texture entries use a dedicated texture reader rather than the generic reflected field-count form. The entry stores the non-null marker and runtime type string, then the texture name, a `linear` boolean, an `i32 anisoLevel` for outer package versions 6 and newer (older versions default it to 1), an `i32` byte count, and that many image bytes. The `CL Wolf Creek` textures have `linear=false`, `anisoLevel=1`, and PNG image data. This sequence was checked across all 237 type-3 entries: every byte count exactly matches the remaining payload, and every image buffer starts with the PNG signature `89 50 4E 47 0D 0A 1A 0A`.

The Rust type-3 codec preserves the assembly-qualified runtime type and image bytes. Its encode/decode path was round-tripped against all 237 type-3 payloads in the external 105-map corpus, including the pre-version-6 `anisoLevel` gate.

## Type-52 map data payloads

The type-52 data payload does not use this reflected header. IL inspection traces its source: `LoadingManager.SaveSimulationData` writes a temporary file by calling `DataSerializer.Serialize` first for `m_metaData`, then for a new `SimulationManager+Data` object. The map packaging callback adds this file as the `_Data` entry with type `Data` (52). Each call writes a 32-bit format version before its root object, so the payload contains two sequential DataSerializer streams.

Each DataSerializer stream starts with a big-endian `u32` format version. `DataSerializer.WriteUInt32` emits bytes most-significant first, and `Deserialize` reads two big-endian `u16` values to recover its version. This differs from the little-endian package header/index. The beginning of `CL Wolf Creek_Data` is `00 01 B5 84` (`0x0001B584`); `ND Island Hopping_Data` begins `00 00 01 3C` (`0x0000013C`).

The internal DataSerializer versions vary independently of the outer package version. Across the corpus, the ranges are: package v4, `0xF4`–`0x140` (5 distinct values); v5, `0x1A9CE` (1); v6, `0x1A9D3`–`0x1B96F` (7); v7, `0x1B972` (1); v8, `0x1BD57`–`0x1C14F` (6); v9, `0x1C52A`–`0x1C534` (4); and v10, `0xC2`–`0x1CCFB` (5). A reader must track these two version fields separately. The object framing and selected manager schemas are recovered below; most manager-specific payload fields remain to be mapped.

`DataSerializer` also uses its own primitive encoding, distinct from both the package header and .NET `BinaryWriter` strings. IL confirms that `WriteUInt16` and `WriteUInt32` emit most-significant bytes first. `WriteUniqueString` writes `0xFF` for null; otherwise it writes a big-endian 24-bit character count. If every character fits in one byte, bit `0x800000` marks the count and each character is delta-encoded from an initial value of 128 into one byte. Strings containing wider characters use unflagged length and big-endian 16-bit deltas.

Object framing from `WriteObject` and `WriteSharedType` is:

- `0xFF` represents a null object.
- A previously encountered object is represented by its object-list index as a big-endian 24-bit integer. The writer limits these IDs to values below `0xFE0000`, keeping them distinct from the `0xFE` new-object marker.
- A new object begins with `0xFE`, followed by its shared-type reference. Existing shared types use a big-endian `u16` type-list index. A new shared type is encoded as `0xFE` followed by a `WriteUniqueString` assembly-qualified type name.
- New objects are added to an object list and later serialized in that list's order through `IDataContainer.Serialize`. This preserves references and allows the root object's serialization to enqueue further objects.

The first bytes of `CL Wolf Creek_Data` after its `u32` version are `FE FE 80 ...`. This matches a new root object, a new shared type, and the flagged compact-string form. Decoding that string yields `SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null`. The next fields also match `SimulationMetaData.Serialize`: the game-instance identifier is `7a8bf7ba-d1dc-45ed-b96b-93e75f9c7130`, the environment is `North`, and the scenario asset/name are null. Continuing with the field sequence from IL parses the complete metadata object through byte 192. A second big-endian version word `0x0001B584` begins at payload offset 193, immediately followed by another `FE FE 80 ...` object/type header. This identifies the boundary between the two Serialize calls in this sample and confirms the second call uses the same version. Its root type is `SimulationManager+Data, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null`.

The Rust `DataSerializerStream` reader handles an isolated stream's big-endian version and new-root/new-shared-type header, including compact and UTF-16 unique strings. `from_parts` writes this same root framing and preserves caller-supplied root field bytes verbatim; decoded streams also retain and re-emit their original bytes. `SimulationMetaData::decode_from` parses the first type-52 root and returns its consumed byte count. On the committed `CL Wolf Creek` sample it consumes exactly 193 bytes, so the second stream starts at the previously established offset. The fixture `simulation-metadata-v6.bin` is exactly bytes 0–192 of that map's `_Data` asset; it is a stream fixture, not a complete `.crp` file.

The field schema below is confirmed from `Assembly-CSharp.dll`, methods `SimulationMetaData.Serialize` and `SimulationMetaData.Deserialize`, and is checked against the version-6 fixture. The writer's field order is: game-instance identifier and environment (unique strings); scenario asset and name (unique strings); scenario-won and scenario-failed booleans; starting `DateTime` ticks (`i64`) and map name; a signed mod-override count followed by unique-string keys and boolean values; traffic-inversion and achievements-disabled bytes; current `DateTime` ticks (`i64`); current day-hour (`f32`); new-map and new-game application versions (`u32`); save application version and data-format version (`u32`). The current DLL's serializer writes that complete sequence. Its deserializer has legacy gates, which the Rust codec follows when reading or rewriting older internal versions:

| Field | DataSerializer version gate | Older-version behavior |
| --- | ---: | --- |
| Game-instance identifier | 161 | Not stored below 161 |
| Environment | All observed versions | Always read |
| Scenario asset | 292 | Null below 292 |
| Scenario name and outcome bytes | 290 | Null / false / false below 290 |
| Starting date and map name | 322–108999, or 109001+ | Default ticks and null name otherwise |
| Mod overrides | 298 | Null collection below 298 |
| Traffic inversion | All observed versions | Always read |
| Achievements-disabled byte | 192 | False below 192 |
| Current date | 183 | Current time below 183 |
| Current day-hour | 212 | Defaults to 12.0 below 212 |
| New-map and new-game app versions | 197 | Both default to 1000000 below 197 |
| Save app and data-format versions | 191 | Save app defaults to 50500; data format defaults to the stream version |

The gap at version 109000 is present in the game's IL condition: the starting-date/map-name fields are skipped at exactly 109000. All 105 available maps have inner DataSerializer versions at least 194. Synthetic tests cover versions 194, 300, and 109000; the version-6 sample and corpus cover later layouts. The opt-in corpus test decodes and re-encodes the first root and the `SimulationManager+Data` root in every type-52 asset it encounters, while preserving the queued manager-object graph unchanged.

`SimulationManager+Data.Serialize` reveals the second block's structure. It writes current tick, frame, and build indices as `u32`, a randomizer seed as `u64`, a day-time offset as `u32`, then the build-index history. It next writes an object array assembled from registered `ISimulationManager.GetData()` results. Finally, it writes a count followed by key/byte-array pairs from `m_serializableDataStorage`.

`SimulationManager.Awake` allocates `m_buildIndexHistory` as 32 `u32` elements; `SimulationManager+Data.Serialize` writes all 32 without a count. In the `CL Wolf Creek_Data` second stream, a leading shared-type declaration (`FE` followed by unique string `SimulationManager`) appears between the root type header and scalar fields. This framing is present in the available corpus; the Rust codec preserves it and registers the name before parsing manager references. The manager array starts with a big-endian 24-bit count; each element is null, an existing 24-bit object ID, or a new-object marker with a shared-type reference. New manager objects are queued and their serialized bodies follow the root fields. The Rust `SimulationManagerData` codec decodes and re-encodes the scalar fields, 32-entry history, manager references, and serializable-data entries, while copying queued manager-object body bytes unchanged. A 3,074-byte fixture cut from `CL Wolf Creek_Data` ends after the second root's serializable-data storage and omits the large queued manager graph.

The matching `Deserialize` method exposes compatibility gates on the internal format version: versions below 37 are rejected; current tick appears at 84, frame at 5, build index at 13, randomizer seed at 10, day-time offset at 212, build-index history at 164, manager data as an object array at 41, and mod serializable-data storage at 186. For older versions the reader supplies defaults; versions below 41 use a legacy sequence of individual manager objects. The typed Rust root codec currently supports version 164 and newer because those versions contain the fixed build-index history. All observed map payload versions are at least `0xC2` (194), so they use the object-array, history, and serializable-storage paths; versions below 212 use a derived day-time offset.

The object-body boundary and order are confirmed by `DataSerializer.Deserialize` and `DataSerializer.Serialize` in `ColossalManaged.dll` (Deserialize RVA `0x488D0`, Serialize RVA `0x4881C`; `ReadObject` RVA `0x47D54`). Deserialize reads the root object, then calls `IDataContainer.Deserialize` on `m_objectList` in increasing list-index order; newly allocated objects from `ReadObject` are appended to that same list and are processed later in FIFO order. Serialize calls each object's `Serialize` in the same list order after writing the root. There is no per-object byte length or boundary marker around these bodies. For the type-52 map roots examined (including the 105-map local corpus), the root manager array starts with `GameAreaManager+Data`, then `TerrainManager+Data`, followed by `WaterSimulation+Data`; therefore their bodies immediately follow one another in that order, with any nested objects queued later. This confirms which body starts at the manager graph, but it does not by itself validate a parser's field lengths within each body.

The Rust `GameAreaManagerData::decode_prefix` decodes the first queued `GameAreaManager+Data` body: area count, version-gated start tile, its fixed 25-entry byte grid, the raw `m_areaNotUnlocked` object-reference token, version-gated buildable-area floats, and version-gated saved camera view. It returns the consumed length, exposes decoded values, and retains the original body for byte-exact re-emission. `SimulationManagerData::decode_game_area_prefix` checks the first manager reference's runtime type before dispatching.

The first-body codec was checked against a 22-byte fixture cut from `CL Wolf Creek_Data` (inner DataSerializer version 112004). The optional external corpus test also decodes the first manager body from each of the 105 local type-52 map assets. In this sample, `EncodedArray.Byte` advertises a repeat run longer than the remaining grid; the game reader simply stops when its preallocated 25-entry array is full. The Rust decoder follows that observed reader behavior. This is a corpus-verified behavior, not a guarantee for every future package.

`TerrainManagerData::decode_prefix` now reads the next queued manager body after the game-area body. `TerrainManager.Awake` allocates 1,168,561 raw-height entries; the Rust codec decodes that `EncodedArray.UShort` array, then decodes one block-height delta for each nonzero raw height (or supplies zeros below version 96). It applies the version-243 dirt-buffer default and skips the three object-reference frames present at version 244 and newer. The original terrain body is retained for byte-exact re-emission, and `SimulationManagerData::decode_terrain_prefix` checks that the second manager reference is `TerrainManager+Data`. The optional 105-map corpus run decoded both leading manager bodies in every type-52 asset. That run is a parser smoke check only: manager bodies are copied raw during re-emission, and the test does not compare decoded terrain values or the next-object boundary with independent expected values. Tests for the UShort literal, repeat, and adjusted-delta controls use independent expected values; no full terrain body is committed as a fixture because the grid is large.

An independent Python implementation of `EncodedArray.UShort.Read` was applied to the queued manager bytes for `CCMP2 Boreal Map 1`, `CCMP2 Desert Map 1`, and `CCMP2 Euro Map 1` (inner versions 116019, 116020, and 116019). It reproduced the Rust decoder's array spans and total Terrain prefix lengths exactly. From the manager-graph start, the raw-height arrays occupy 1,201,258, 1,398,249, and 1,208,418 bytes; the sparse block-delta arrays occupy 47,420, 49,109, and 36,020 bytes. The independently located dirt-buffer fields and three following three-byte object references end at graph offsets 1,248,743, 1,447,423, and 1,244,503, matching the Rust prefix lengths. This cross-check supports the encoded-array byte counts for these samples; it is not a game-produced field-value oracle and does not validate Water's own sparse counts.

A separate audit of the 105 local maps found that parser success alone is not enough to establish field-level correctness. In `CCMP2 Boreal Map 1` (inner version 116019), the current GameArea prefix reader reports extreme floating-point camera values; the current Terrain prefix reader reports `m_dirtBuffer` bytes `D7 42 03 FA`. The camera values remain unexplained. The reported GameArea object reference (`80 00 0F`) and Terrain references are outside the current DataSerializer object-list bounds if interpreted at those offsets, but `ReadObject` explicitly warns and returns null for an out-of-bounds ID while consuming the fixed three-byte reference. Such IDs alone therefore do not prove that the field boundary is wrong. The IL confirms manager body order and version-gated field order, but the typed Terrain values and the later WaterSimulation start have not been independently validated against decoded expected values. The original manager graph remains preserved byte-for-byte.

`WaterSimulation+Data.Serialize` and `Deserialize` are mapped from `Assembly-CSharp.dll` (`Deserialize` RVA `0x316730`). `WaterSimulation.Awake` allocates two buffers of `(1080 + 1)^2 = 1,168,561` cells. The data body stores a full `EncodedArray.UShort` height grid, then sparse `UShort` pollution values only for nonzero-height cells. From version 42, it stores sparse `EncodedArray.Short` horizontal velocity for wet cells or dry cells immediately left of a wet cell, followed by sparse vertical velocity for wet cells or dry cells immediately above a wet cell; earlier versions store one velocity per wet cell. The adjacency tests and row-major indexing come directly from the manager's nested deserialize loops.

After the grids, the body writes a `u16` water-source count and fixed-versioned `WaterSource` records. `WaterSource.Serialize`/`Deserialize` (RVA `0x316DD4`/`0x316E50`) show two `Vector3` positions, a `u16` type and target, and five `u24` values in the version-184+ layout (43 bytes total). Before 184, the output position is copied from the input position and older rate/value fields follow separate version-95 gates. `WaterSimulation+Data.Deserialize` reads water waves from version 259; `WaterWave.Serialize`/`Deserialize` (RVA `0x318590`/`0x318630`) use a `u16` type from version 261 and a boolean before that. The version-261+ wave record is 24 bytes (12 two-byte fields); the earlier boolean-type layout is 23 bytes. The tail contains sea level (`f32`, version 131), two pollution rates (`i8`, version 147), and water frame index (`u32`, version 173). Earlier versions use defaults for gated fields.

This WaterSimulation schema is IL-confirmed, but its large sparse-array and source-list boundaries have not yet been validated across the map corpus or implemented in Rust. Applying the IL-derived sparse counts at the current TerrainManager prefix boundary produces source counts of 59,951 in `CCMP2 Boreal Map 1` (inner version 116019), 65,535 in `CCMP2 Desert Map 1` (116020), and 16,673 in `CCMP2 Euro Map 1` (116019). At 43 bytes per version-184+ source, the Boreal count needs 2,577,893 bytes after its count field, exceeding the entire remaining manager graph by 47,127 bytes; Desert needs 2,818,005 bytes and exceeds the remaining graph by 1,092,582 bytes. Euro's record span fits, but its first records decode to extreme/denormal position values, and its following wave count is 52,942 with a sea-level value around -2.08e36. Therefore none of these current-boundary parses yields a valid WaterSimulation tail. An earlier byte scan found a Boreal candidate 1,047 bytes later with 57 source records and a plausible tail (sea level 38, rates 1/1, frame 4653876), but this candidate was not reproduced as a consistent boundary across the three-map follow-up; treat it as exploratory.

A corpus-wide diagnostic applied the same IL-derived grid counts and version gates at the current Terrain prefix boundary to all 105 local maps. All 105 third manager references were `WaterSimulation+Data`. The inner serializer versions range from 194 to 118011. For 48 maps, the parsed source count alone required more bytes than remained in the serialized manager graph. For another 40, source records fit, but the following wave/tail fields failed a deliberately loose plausibility check (finite sea level with absolute value below 200 and frame below 100 million). The remaining 17 passed only that loose check; this is not validation, since the check does not establish valid source coordinates or physically sensible wave fields. These outcomes occur across both the older versions below the wave-table gate (including 244) and later versions through 118011, so this audit does not isolate a version band that explains the mismatch. It does show that the current Terrain boundary plus the current Water sparse-array interpretation is not a usable corpus-validated Water decoder. The result is diagnostic, not proof that the samples are malformed: queued object bodies have no individual length markers, and the boundary or a version-dependent field interpretation may still be wrong.

An independent Terrain array-span check above matches the Rust prefix lengths and IL field order for three samples, so a Terrain UShort byte-count error is less likely but not ruled out corpus-wide. The source-list and tail boundary remains unresolved. Keep the WaterSimulation payload opaque until its decoded fields and following object boundary agree with the DLL across multiple game-produced samples. The DLL contains additional `IDataContainer` implementations for `NetManager+Data`, `BuildingManager+Data`, `CitizenManager+Data`, `PropManager+Data`, `VehicleManager+Data`, `ZoneManager+Data`, and others. The complete simulation graph is much larger than the two roots; Rust currently decodes the first two manager bodies and preserves the remaining graph.

A corpus audit revisited the `GameAreaManager+Data` end offset because its decoded floats looked implausible. The IL field order and version branches in `GameAreaManager+Data.Deserialize` (RVA `0x225F18`) predict consumed lengths of 6 bytes when start-tile, reference, buildable-area, and camera gates are active only through the reference; 22 bytes when the four buildable-area floats are also present; and 52 bytes when the inline 30-byte saved-camera view is present. Across the 110 type-52 map assets in the 105-file local corpus, the decoder returned exactly those three lengths: 3 assets at 6 bytes, 45 at 22, and 62 at 52. This is consistent with the IL version gates (start tile 137, reference 87, buildable area 199, camera view 112032) and gives a corpus-wide byte-span check for the end of GameArea, though it is not an independent game-produced field-value oracle.

The suspicious values also repeat as byte patterns rather than varying with nearby field alignment: 107 of the 110 assets that include buildable-area floats decode to the same four values (`[-39436.97, -2.0035734e17, -7.159476e26, 9.556328e-35]`), and all 62 assets that include the saved camera view decode the same seven position/size/angle/height values. The 48 older assets without that camera gate return the IL-specified default mode `-1` and zero values. The repeated bytes explain why these values are poor evidence for shifting the GameArea boundary; their intended semantic meaning remains unknown. In all 110 assets, the area count and start tile decode as zero, and the 25-entry encoded grid decodes as all zeroes. The `m_areaNotUnlocked` token is consistently `80 00 0F`; `DataSerializer.ReadObject` interprets this as object-list index `0x80000F`, warns that it is out of bounds for the observed graph, and returns null while consuming the three-byte token. Together, the IL field sequence, version-dependent consumed lengths, and repeated grid/reference byte spans support the current GameArea end offset. They do not validate the typed Terrain values or locate Water's boundary, so the Water investigation remains open.

A further Terrain-end check compared the current codec's consumed length with a second cursor that re-read the two encoded UShort array spans and followed the DLL's object-reference framing at the end of Terrain. Both cursors ended at the same byte in all 110 type-52 assets (versions 194 through 118011). In particular, `DataSerializer.ReadObject` treats `0xFE` as a new object and `ReadSharedType` then consumes either a unique string or a two-byte shared-type index; the Rust Terrain reference skipper consumes all three bytes in the indexed case, so the suspected one-byte under-read is not present. This supports the current Terrain-to-Water start offset at the byte-framing level. It does not validate Water's sparse velocity values or source-list boundary; those remain the unresolved part of this research.

The next queued body provides an independent boundary check. In all 105 local map roots, the first four manager references are, in order, `GameAreaManager+Data`, `TerrainManager+Data`, `WaterSimulation+Data`, and `InstanceManager+Data`. This ordering follows the root object IDs and `DataSerializer.Deserialize`'s FIFO processing described above. `InstanceManager+Data.Serialize` / `Deserialize` in `Assembly-CSharp.dll` (RVA `0x22E21C` / `0x22E344`) define a recognizable body prefix: a big-endian `u32` name count followed by that many `InstanceID` (`u32`) and unique-string pairs; `InstanceID.Serialize` / `Deserialize` are at RVA `0x22C97A` / `0x22C988`. The normal deserialize branch reads a second `u32` count and `InstanceID` / object-reference pairs from version 258 onward. A bounded structural check of the next body after each Water tail that fit the graph found no valid InstanceManager prefix among those 48 maps. This independently rejects those current Water endpoints as confirmed boundaries, though it does not locate the correct one: the Water spans in the other 57 maps already overrun the remaining graph, and the 48 checked endpoints can still be misaligned because their source/tail values are not all plausible.

A shifted-start search then used a longer queue signature: candidate Water grids, source/wave counts and tail; a bounded `InstanceManager+Data` names/groups table; the fixed 262,143-entry `PathManager+Data` simulation-flags array and its active path records; then the opening lane/node arrays from `NetManager+Data`. On `CCMP2 Boreal Map 1` (version 116019), every Water start shift from 0 through 2,048 bytes was tried; shifts through 8,192 bytes were also sampled every eight bytes on Boreal, `CCMP2 Desert Map 1` (116020), and `CCMP2 Euro Map 1` (116019). No candidate passed the complete chain. The Water-tail filter only required a bounded source span, wave count at most 10,000, finite sea level with magnitude below 200, and frame below 100 million; it did not validate source coordinates. The search therefore finds no confirmed boundary and does not rule out a larger shift, a shift before the current Terrain prefix, or a remaining version/schema error. Several isolated InstanceManager/Path-shaped byte sequences failed at the subsequent NetManager prefix and are treated as false matches.

A cross-grid probe compared `TerrainManager.rawHeights` and the first 1,168,561 values decoded as `WaterSimulation.m_height` from the current prefix boundary in those same three maps. The Terrain grids contain 1,125,932, 1,168,561, and 1,168,560 nonzero cells respectively; the candidate Water grids contain 90,166, 46,341, and 240,097. Exact cell equality is only 13,963, 0, and 2 cells respectively. This is not an alignment check by itself: IL treats the arrays as separate simulation buffers, and `WaterSimulation.SimulateWater` (RVA `0x313170`) clamps water heights to the full `u16` range, including 65,535. Neither exact equality nor saturation can certify or reject the candidate start. The comparison therefore supplies no independent support for the current Water boundary; it also does not identify a common shifted boundary.

Two map-critical manager schemas are now mapped at the IL level. `GameAreaManager+Data.Serialize` writes `m_areaCount` (`u8`), `m_startTile` (`u8`), the known-length `m_areaGrid` through `EncodedArray.Byte`, `m_areaNotUnlocked` as a DataSerializer object, four buildable-area floats, and a serialized `SavedCameraView`. Its reader gates are: start tile at version 137 (otherwise defaults to 12), `m_areaNotUnlocked` at 87 (otherwise null), the four area floats at 199 (otherwise all default to -1), and saved camera view at 112032 (otherwise its mode defaults to -1).

`TerrainManager+Data.Serialize` writes the known-length raw-height array through `EncodedArray.UShort`, then a second `EncodedArray.UShort` for nonzero block-height differences (`m_blockHeights - m_blockHeights2`) at positions whose raw height is nonzero. It then writes `m_dirtBuffer` (`i32`) and three DataSerializer objects for the terrain-tool / dirt-warning state. The reader always loads raw heights; it loads the block-height deltas from version 96 (otherwise zeroes them), `m_dirtBuffer` from 243 (otherwise defaults to 524288), and the three objects from 244 (otherwise null).

`EncodedArray.Byte` and `EncodedArray.UShort` are inner array codecs, not package-wide compressors. They emit control blocks with up to 128 values; the decoder gets the total element count from the manager's preallocated array.

For `Byte`, control `0x00`–`0x7F` means the next `control+1` bytes are literal values. Control `0x80`–`0xFF` repeats the previous signed byte delta `control-0x7F` times; each result is previous value plus that delta modulo 256.

For `UShort`, control `0x00`–`0x3F` means the next `control+1` big-endian `u16` values are literals. Control `0x40`–`0x7F` means `control-0x3F` signed-byte adjustments to the previous delta; each adjusted delta is added to the previous value. Control `0x80`–`0xFF` repeats the previous signed 16-bit delta `control-0x7F` times. Reconstructed values wrap to 16 bits. Both codecs initialize the previous value and delta to zero. The DLL defines corresponding encoded-array helpers for signed/unsigned integers, floats, booleans, and 64-bit values; their exact codecs have not been analyzed here.

## Findings from the DLLs

1. The format is a package container rather than a single raw map image: the libraries include dedicated package readers and writers, an index table, and a collection of entries.
2. The container has a versioned header and an index layout confirmed above. The supplied corpus contains multiple versions, so a Rust reader should preserve and explicitly validate the version number.
3. `Package.Save` copies asset payload bytes into the package; no generic compression layer was found in the container writer. Type-3 assets contain PNGs, and specific DataSerializer array fields use delta/RLE encoding; other fields may have their own encodings.
4. Entry checksums are lowercase hexadecimal MD5 hashes of the raw asset payload. `SecurityHelper.CalculateChecksum` computes MD5 and formats it as lowercase hex; the `Package.Asset` constructors call the byte-array or file overload. Recomputing hashes for all 469 entries across the 105 supplied maps matched every stored checksum.
5. The code separates container handling (reader, writer, index, entries) from game-object serialization (`PackageSerializer`, `DataSerializer`). The type `100` metadata uses the reflected-object path; type `52` uses `DataSerializer` object framing, while the game-specific `IDataContainer` payloads remain to be mapped.

## Open questions

- Any version-specific changes to index fields.
- Whether data offsets and entry sizes require additional handling for all package implementation types.
- The remaining manager-specific data field sequences.
- Semantic differences between versions 4–10.

## Next research steps

1. Decode the remaining manager-specific `IDataContainer` payloads, beginning with network, building, and citizen data.
2. Confirm the shared format rules on non-map package families.
3. Compare field-level behavior across the observed inner serializer versions before defining the public Rust API.

## Limitations of this pass

No high-level .NET decompiler (`monodis`, `ilspycmd`, or `ildasm`) was found, so reader, writer, and serializer methods were decoded from CIL bodies and metadata with a small inspection script. The outer container, MD5 checksums, reflected metadata headers, complete type-3 texture framing, and second DataSerializer root headers were cross-checked against all 105 map files. A full field-level decoder for every simulation manager and other package implementation types remains outside the current findings.
