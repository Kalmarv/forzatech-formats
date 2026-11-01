# Changelog

## Unreleased

- First version: Forza Horizon 6's encrypted container (header, blocks, MACs, decryption with keys you supply), the
  keys file, and the game database's keystream.
- `minizip`: the PGZP streaming archives (`GeoChunkN.minizip`): header, folder index, entries, decoding (stored, raw
  DEFLATE, raw LZ4), the chunk map and the contents list.
