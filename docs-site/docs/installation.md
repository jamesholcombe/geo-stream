---
sidebar_position: 2
---

# Installation

```bash
npm install @jamesholcombe/geo-stream
```

Pre-built native binaries are included in the package — no Rust toolchain is required.

## Supported platforms

| Platform | Architecture |
|----------|-------------|
| macOS | arm64 |
| macOS | x64 |
| Linux (gnu) | x64 |
| Linux (gnu) | arm64 |
| Windows | x64 |

## Node.js requirement

Node.js **18 or later** is required.

## Importing

```typescript
import { GeoEngine, GeoEvent, GeoJsonPolygonInput } from '@jamesholcombe/geo-stream/types'
```

The `/types` entry point provides typed wrappers with a discriminated union for `GeoEvent`. Importing from the root index (`@jamesholcombe/geo-stream`) gives the raw NAPI bindings where `ingest()` returns `unknown[]`.

## TypeScript configuration

Recommended `tsconfig.json` settings:

```json
{
  "compilerOptions": {
    "moduleResolution": "node16"
  }
}
```

`"moduleResolution": "bundler"` also works. Avoid the legacy `"node"` setting — it does not resolve subpath exports correctly.
