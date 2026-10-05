# Preliminary research on the `.crp` format

Status as of 2026-10-05. These notes are based on the game DLLs and samples in `data/maps/`. They record current observations, not a complete implementation specification.

## Rust implementation status

The crate has an outer-container reader and compact writer. `Package::read_from` reads package versions 4–10 using the base implementation's zero data offset; `Package::read_from_with_data_offset` accepts an implementation-specific offset. The reader retains index order, checksum strings, unknown numeric asset types, and raw payload bytes, and rejects payloads whose MD5 does not match the index. `Package::write_compact_to` emits assets in index order with recalculated contiguous offsets and MD5 checksums. It preserves the stored author field without decrypting or encrypting it.

`MapMetaData::decode` and `encode` support the 25-field reflected type-100 `MapMetaData` schema, including mod entries and trailing bytes. Other reflected metadata runtime types remain raw. `TextureAsset::decode` and `encode` support type-3 image wrappers, including the outer-version-6 anisotropic-level gate. Image bytes remain opaque and are preserved. `SimulationMetaData::decode_from` and `encode` handle the first type-52 DataSerializer root and return its consumed length. `SimulationManagerData::decode` and `encode` handle the second root's scalar fields, fixed build-index history, manager reference array, and mod serializable-data storage; the GameArea, Terrain, and Water prefixes can be decoded. Original queued bytes remain available for exact rewriting; `WaterSimulationEditor` also supports explicit edits to Water grids, sources, sea level, pollution rates, and frame index.

The default `cargo test --offline` suite uses small committed fixtures cut from game maps, covering metadata from package versions 4 and 10, a texture from version 6, an isolated 193-byte SimulationMetaData stream, and a 3,076-byte SimulationManager root-field fixture and a 108-byte GameArea body from version 6, plus a 72,503-byte WaterSimulation body from `CCMP2 Desert Map 1`. It includes MD5 vectors, synthetic round-trips for outer versions 4–10, malformed inputs, typed metadata and texture round-trips, both DataSerializer root codecs, and exact writer index bytes. An ignored test can be run with `CRP_CORPUS_DIR` to parse and checksum a full external corpus and round-trip every type-3 texture, type-100 metadata asset, and both type-52 roots while decoding the GameArea, Terrain, and Water prefixes and preserving the manager object graph. Full map files are not required by or included in the default test suite.

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

`SimulationManager.Awake` allocates `m_buildIndexHistory` as 32 `u32` elements; `SimulationManager+Data.Serialize` writes all 32 without a count. The manager array starts with a big-endian 24-bit count; each element is null, an existing 24-bit object ID, or a new-object marker with a shared-type reference. New manager objects are queued and their serialized bodies follow the root fields. The Rust `SimulationManagerData` codec decodes and re-encodes the scalar fields, history, manager references, and serializable-data entries, while copying queued manager-object bytes unchanged. The `CL Wolf Creek_Data` root fixture is 3,076 bytes and includes the root's closing profiler tag; it omits the queued manager graph.

The matching `Deserialize` method exposes compatibility gates on the internal format version: versions below 37 are rejected; current tick appears at 84, frame at 5, build index at 13, randomizer seed at 10, day-time offset at 212, build-index history at 164, manager data as an object array at 41, and mod serializable-data storage at 186. For older versions the reader supplies defaults; versions below 41 use a legacy sequence of individual manager objects. The typed Rust root codec currently supports version 164 and newer because those versions contain the fixed build-index history. All observed map payload versions are at least 194, so they use the object-array, history, and serializable-storage paths.

### Corrected manager framing and Water boundary

**Correction to the earlier Water investigation:** the byte `FE` and string `SimulationManager` before the root's scalar fields are a `LoadingProfiler.BeginSerialize` shared-string tag, not a DataSerializer shared-type declaration. `LoadingProfiler.BeginSerialize` / `EndSerialize` (Assembly-CSharp.dll RVA `0x261787` / `0x2617AD`) write these tags around each manager body. The matching deserialize methods (RVA `0x2617D4` / `0x261838`) consume them for inner versions at least 130. `DataSerializer.WriteSharedString` / `ReadSharedString` (ColossalManaged.dll RVA `0x47A38` / `0x47AB4`) maintain a shared-string table separate from the shared-type table used for object references. A new string is `FE` plus a unique string; an existing one is a big-endian `u16` table index. The root closing tag is `00 00` (shared-string index 0). The earlier parser incorrectly treated this closing tag and the following `GameAreaManager` opening tag as GameArea fields, shifting every later boundary. All old diagnostics of implausible GameArea floats, Terrain values, and Water source counts were consequences of this framing error and are superseded by the results below.

`DataSerializer.Deserialize` / `Serialize` in ColossalManaged.dll (RVA `0x488D0` / `0x4881C`) process queued object bodies in object-list order after the root. The first four references in the examined map roots are `GameAreaManager+Data`, `TerrainManager+Data`, `WaterSimulation+Data`, and `InstanceManager+Data`. Each body has a profiler opening tag, its fields, and a matching closing tag. In `CCMP2 Boreal Map 1`, after the corrected root boundary, the manager graph starts with `FE 80 00 0F` plus the compact string `GameAreaManager`; its body then starts with area count 0, start tile 7, and `0x98`, a valid 25-cell repeated-zero area grid. The GameArea body ends at graph offset 159 including its tags. The next tag is `TerrainManager`, and the Terrain body ends at offset 1,248,864 including its tags. The next tag is `WaterSimulation`, so the Water body starts at graph offset 1,248,883, after its 19-byte opening tag. The same tag sequence was checked in `CCMP2 Desert Map 1`, `CCMP2 Euro Map 1`, `MP27_Map07`, and `CL Wolf Creek`.

The corrected GameArea fields are plausible and differ by map. The four buildable-area floats are approximately `[0.858, 0.838, 0.783, 0.765]` in Boreal, `[0.840, 0.886, 0.803, 0.759]` in Desert, and `[0.907, 0.786, 0.674, 0.614]` in `CL Wolf Creek`. The latter's complete 108-byte GameArea body is a committed fixture. The raw `m_areaNotUnlocked` reference begins `FE FE` and declares `GenericGuide`, explaining why the previous three-byte `80 00 0F` interpretation was false. The previously committed 22-byte GameArea fixture was cut at the wrong position and has been replaced.

`WaterSimulation+Data.Serialize` / `Deserialize` in Assembly-CSharp.dll (RVA `0x3163A8` / `0x316730`) write a fixed 1,168,561-cell `EncodedArray.UShort` height grid, then `UShort` pollution values only for wet cells. At version 42 and newer, `EncodedArray.Short` horizontal velocity is present for wet cells and dry cells immediately left of a wet cell; vertical velocity is present for wet cells and dry cells immediately above a wet cell. These are row-major arrays with side length 1081. Before version 42 each velocity array has one value per wet cell. The Rust Water decoder accepts inner versions 184 and newer and retains the original encoded bytes for exact rewriting. `WaterSimulationData::into_editor` creates an explicit editable view; its writer regenerates the four arrays using valid literal and repeated-delta controls, then writes source records and the version-gated tail. `SimulationManagerData::replace_water` splices only that body back into the manager graph, retaining profiler tags and later manager bytes.

The grid arrays are followed by a big-endian `u16` water-source count. `WaterSource.Serialize` / `Deserialize` (RVA `0x316DD4` / `0x316E50`) write two `Vector3` positions, `u16` type and target, and five 24-bit parameters in the version-184+ layout, totaling 43 bytes per source. Before 184, source layout and rate/value gates differ and are not yet decoded by Rust. A wave list appears at version 259; `WaterWave.Serialize` / `Deserialize` (RVA `0x318590` / `0x318630`) write 24 bytes per wave from version 261 and 23 bytes before 261. Rust bounds and preserves wave bytes during both raw and edited writes; it does not yet expose typed wave fields. The tail is sea level (`f32`, version 131), two signed pollution rates (`i8`, version 147), and water frame index (`u32`, version 173). `DataSerializer.ReadFloat` (RVA `0x476D8`) and `ReadVector3` (RVA `0x47770`) confirm big-endian float values.

A typical edit preserves the first type-52 root and replaces only the second root's Water body:

```rust
let asset = package.assets.iter_mut()
    .find(|asset| asset.asset_type == 52)
    .ok_or_else(|| std::io::Error::other("map has no type-52 asset"))?;
let (_, manager_start) = SimulationMetaData::decode_from(&asset.data)?;
let mut manager = SimulationManagerData::decode(&asset.data[manager_start..])?;
let (water, _) = manager.decode_water_prefix()?;
let mut editor = water.into_editor();
editor.set_sea_level(42.0);
manager.replace_water(&editor)?;
let mut payload = asset.data[..manager_start].to_vec();
payload.extend(manager.encode()?);
asset.data = payload;
package.write_compact_to(&mut output_file)?;
```

The type-52 asset checksum is recalculated by `Package::write_compact_to` from the newly encoded raw payload.

The complete 72,503-byte Desert Water body is also a committed fixture, with independently recorded source count, wet-cell count, sea level, pollution rates, and frame index checked by the default test suite. The following values were decoded from complete game-produced Water bodies. Graph offsets include profiler tags and start immediately after the corrected SimulationManager root. The Water end is after its two-byte closing tag `00 03`; at that position each sample starts with `FE 80 00 0F` and the compact string `InstanceManager`.

| Map | Inner version | Water body start | Source count | Waves | Sea level | Water frame | Water end |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| CCMP2 Boreal Map 1 | 116019 | 1,248,883 | 57 | 0 | 38 | 4,653,876 | 1,600,404 |
| CCMP2 Desert Map 1 | 116020 | 1,447,487 | 29 | 0 | 35 | 3,420,902 | 1,519,992 |
| CCMP2 Euro Map 1 | 116019 | 1,244,546 | 15 | 0 | 40 | 3,488,448 | 1,417,673 |
| MP27_Map07 | 118011 | 1,031,547 | 7 | 0 | 40 | 8,908,607 | 2,192,909 |
| CL Wolf Creek | 112004 | 1,754,472 | 4 | 0 | 67.76501 | 26,859,392 | 2,000,148 |

The opt-in local corpus test also decoded GameArea, Terrain, and Water in all 110 type-52 assets across 105 map files (inner versions 194 through 118011) and checked the next `InstanceManager` opening tag. These corpus observations validate the boundaries for those samples; they are not claims about every possible `.crp` asset. The `InstanceManager+Data` schema in Assembly-CSharp.dll (Serialize / Deserialize RVA `0x22E21C` / `0x22E344`) begins with a big-endian name count. Its opening profiler tag provides an independent cross-manager boundary check. An unedited queued graph is preserved byte-for-byte. An explicit edited Water write can change its encoded-array run grouping and body length; it leaves other manager bodies unchanged. The writer rejects nonzero pollution or velocities outside their sparse masks, source counts over 65,535, and source parameters over 24 bits rather than dropping values. The complete graph contains further managers such as Net, Building, Citizen, Prop, Vehicle, and Zone, which remain opaque.

Two map-critical manager schemas are now mapped at the IL level. `GameAreaManager+Data.Serialize` writes `m_areaCount` (`u8`), `m_startTile` (`u8`), the known-length `m_areaGrid` through `EncodedArray.Byte`, `m_areaNotUnlocked` as a DataSerializer object, four buildable-area floats, and a serialized `SavedCameraView`. Its reader gates are: start tile at version 137 (otherwise defaults to 12), `m_areaNotUnlocked` at 87 (otherwise null), the four area floats at 199 (otherwise all default to -1), and saved camera view at 112032 (otherwise its mode defaults to -1).

`TerrainManager+Data.Serialize` writes the known-length raw-height array through `EncodedArray.UShort`, then a second `EncodedArray.UShort` for nonzero block-height differences (`m_blockHeights - m_blockHeights2`) at positions whose raw height is nonzero. It then writes `m_dirtBuffer` (`i32`) and three DataSerializer objects for the terrain-tool / dirt-warning state. The reader always loads raw heights; it loads the block-height deltas from version 96 (otherwise zeroes them), `m_dirtBuffer` from 243 (otherwise defaults to 524288), and the three objects from 244 (otherwise null).

`EncodedArray.Byte` and `EncodedArray.UShort` are inner array codecs, not package-wide compressors. They emit control blocks with up to 128 values; the decoder gets the total element count from the manager's preallocated array.

For `Byte`, control `0x00`–`0x7F` means the next `control+1` bytes are literal values. Control `0x80`–`0xFF` repeats the previous signed byte delta `control-0x7F` times; each result is previous value plus that delta modulo 256.

The Water editor currently emits only `0x00`–`0x3F` literal controls (up to 64 values) and `0x80`–`0xFF` repeated-delta controls (up to 128 values). This is a documented canonical encoding choice; it is not a claim to reproduce the DLL writer's byte-for-byte run grouping. The same bit-level codec is used for signed `Short` velocities, interpreted as two's-complement `i16` values. Synthetic expected-byte tests cover literal and repeated-delta controls, signed velocities, sparse-mask boundaries, source records, wave preservation, and an edited type-52 asset written through the package writer. The opt-in local corpus test also decodes canonically re-encoded Water bodies and compares every grid and source field across the available maps.

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
