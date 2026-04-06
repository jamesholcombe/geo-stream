---
id: installation
title: Installation
sidebar_position: 2
description: Install the npm package, supported platforms, and TypeScript import paths.
---

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
import {
  GeoEngine,
  type GeoEvent,
  type GeoJsonPolygonInput,
} from '@jamesholcombe/geo-stream'
```

The root package exports the typed `GeoEngine` wrapper: `ingest()` returns `GeoEvent[]`, and event kinds form a discriminated union for narrowing. Subpath imports such as `@jamesholcombe/geo-stream/emitter` are available for adapters.

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
