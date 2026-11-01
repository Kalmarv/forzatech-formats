# Changelog

## Unreleased

- First version: Forza Horizon 6's encrypted container (header, blocks, MACs, decryption with keys you supply), the
  keys file, and the game database's keystream.
- `minizip`: the PGZP streaming archives (`GeoChunkN.minizip`): header, folder index, entries, decoding (stored, raw
  DEFLATE, raw LZ4), the chunk map and the contents list.
- `zip`: the PKWARE zip archives (ZIP64 included) under `media`, their stored and DEFLATE entries decoded and their
  encrypted (method 22) entries decrypted with keys you supply, each decode checked against the entry's size and
  CRC-32.
- `settings`: the `DatabaseFile` settings XML of the encrypted render and game settings entries (blocks, properties,
  lists, maps; values as stored), and `settings::root` to tell an XML file's root element.
- `bundle`: the Grub bundles models, textures, materials, shaders, crowds and car colour lists are stored in: the
  header, the blob records, the metadata records (with the escaped size of a payload of 0xFFF bytes or more) and each
  blob's data, every byte of the layout checked, and the bundles nested in blobs read the same way.
- `gamedb::car_wheels`: each car's stock tyre width, aspect ratio and rim diameter, front and rear, and its stock
  wheel, from the decrypted database (read through primase-foundations' `sqlite`).
- `pgeo`: the per-cell placement files (`.pgeo`): header (kind, bounding box, category, route, gameplay tag, season
  exclusion), blocks of 80-byte instance records (fixed-point positions, rotation rows, scale, tint, material variant,
  frame), spline-deform curve records, source ranges, the crowd-spline section, and `Pgeo::instances_in` /
  `splines_in` to ask for the placements in one x/z box (`Header::may_hold` selects files from the header alone).
- `plib`: the instance template libraries (`.plib`): version 38 libraries (draw behaviours, model templates, tailors,
  UV, scale and density entries, materials, placement templates, names) and version 9 group libraries.
- `pvs`: the hand-placed instance file (`.pvs`, version 0xA4): textures, megatextures, bounds, gameplay records,
  index tables, shaders, materials, models, cells, dependencies, frames, transforms, LOD rows and profiles, instances,
  model bounds and the tail, every reference checked against what it refers to; `World::instances_in` for a box.
- `track`: an index of a track's GeoChunk entries from its chunk maps and contents lists, to resolve a `.pvs` model
  number, a cell's reference or a template's model path to the entry that holds it.
- `world`: the placement grid (`Cell::from_path`, `Area`), the fixed-point step, and the season mask.
- `minizip::ChunkMapRecord` now holds the record's 24-bit `id` and `resource_type` (the former `unknown_0` and
  `unknown_2`); `reference()` gives the `type << 24 | id` word the `.pvs` files use.
- Depends on primase-foundations 0.3.1 (features `compress` and `sqlite`) from crates.io, and on quick-xml 0.42.0.
