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
- Depends on primase-foundations 0.3.1 (features `compress` and `sqlite`) from crates.io, and on quick-xml 0.42.0.
