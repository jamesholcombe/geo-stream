# Protocol JSON Schema (draft 2020-12)

Machine-readable shapes for what this repository **accepts and serializes** today: NDJSON stdin/stdout/stderr lines (CLI `geo-stream`). They mirror the serde types in `crates/adapters/stdin-stdout`. Human-readable contract: [`protocol/ndjson.md`](../ndjson.md).

## Files

| Schema | Use |
|--------|-----|
| `geojson-polygon-geometry.schema.json` | Reusable `$defs`: GeoJSON Polygon geometry (`type` + `coordinates`). Structural only; not full topology validation. |
| `ndjson-stdin-line.schema.json` | One **stdin** input object per line (`type`-discriminated). |
| `ndjson-stdout-line.schema.json` | One **stdout** event per line (`event`-discriminated). |
| `ndjson-stderr-line.schema.json` | One **stderr** error object: `{ "error": string }`. |

## NDJSON

Streams are **newline-delimited JSON**: each non-empty line is a single JSON object. Validate by running your chosen validator **once per line** (not on the whole file as one JSON value).

## `$id` and `$ref`

These files **omit** top-level `$id` so common validators resolve relative `$ref` (for example to `geojson-polygon-geometry.schema.json`) from the **on-disk path** of the schema you pass in. If you publish copies under a stable URL, add your own `$id` values that match that base so references resolve there.

## Validating with check-jsonschema

From the repository root (so `--schemafile` paths match CI):

```bash
pip install 'check-jsonschema>=0.28'
SCHEMA=protocol/schema/ndjson-stdin-line.schema.json
while IFS= read -r line || [ -n "$line" ]; do
  [ -z "$line" ] && continue
  echo "$line" | python -m check_jsonschema --schemafile "$SCHEMA" -
done < examples/sample-input.ndjson
```

## Validating with ajv-cli

Use a JSON Schema implementation that resolves relative `$ref` from the schema file directory; point it at the same per-line instances as above.
