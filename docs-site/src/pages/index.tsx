import type { ReactNode } from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import { Highlight, themes } from 'prism-react-renderer';
import styles from './index.module.css';

const CODE_SNIPPET = `import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()
  .registerZone('warehouse', warehousePolygon)
  .registerCircle('depot', 500, 300, 50)  // cx, cy, radius (metres)
  .defineRule('fast-entry', rule =>
    rule.whenEnters('warehouse').speedAbove(15).emit('speeding-alert')
  )

// Feed in position updates — events come back synchronously
const events = engine.ingest([
  { id: 'driver-1', x: 505, y: 298, tMs: Date.now() },
])
// [{ kind: 'approach', id: 'driver-1', circle: 'depot', t_ms: ... }]`;

const FEATURES = [
  {
    icon: '⚡',
    title: 'In-process, zero latency',
    body: 'A native Rust module. No server, no network hop, no round-trip per update. State lives in your process.',
  },
  {
    icon: '🔁',
    title: 'Deterministic',
    body: 'Same inputs always produce the same events in the same order. Replay, backtest, and unit test with exact reproducibility.',
  },
  {
    icon: '⏱️',
    title: 'Dwell and sequences',
    body: 'Debounce noisy boundary crossings with per-zone dwell thresholds. Detect ordered multi-stop routes with sequence rules.',
  },
  {
    icon: '🔌',
    title: 'Adapters included',
    body: 'EventEmitter, Kafka, and Redis Streams adapters ship in the box. Structural typing — no hard deps.',
  },
];

function Hero() {
  return (
    <section className={styles.hero}>
      <div className={styles.heroGlow} />
      <div className={styles.heroGrid} />
      <div className={styles.heroInner}>
        <div className={styles.badge}>Open source · MIT · Node.js 18+</div>
        <h1 className={styles.heroTitle}>
          An embeddable rules engine
          <span className={styles.heroTitleAccent}> for location streams</span>
        </h1>
        <p className={styles.heroSubtitle}>
          Drop it into your Node.js process. Feed it position updates. Receive
          typed spatial events — enter, exit, approach, recede — synchronously,
          with no server, no network calls, and no external dependencies.
        </p>
        <div className={styles.heroActions}>
          <Link className={styles.btnPrimary} to="/docs/">
            Get started →
          </Link>
          <Link
            className={styles.btnSecondary}
            href="https://github.com/jamesholcombe/geo-events"
          >
            GitHub
          </Link>
        </div>
      </div>
    </section>
  );
}

function CodePreview() {
  return (
    <section className={styles.codeSection}>
      <div className={styles.codeCard}>
        <div className={styles.codeBar}>
          <span className={styles.codeDot} style={{ background: '#ff5f57' }} />
          <span className={styles.codeDot} style={{ background: '#febc2e' }} />
          <span className={styles.codeDot} style={{ background: '#28c840' }} />
          <span className={styles.codeBarLabel}>geo-stream · quickstart.ts</span>
        </div>
        <Highlight theme={themes.nightOwl} code={CODE_SNIPPET} language="typescript">
          {({ style, tokens, getLineProps, getTokenProps }) => (
            <pre className={styles.codePre} style={{ ...style, background: 'transparent' }}>
              {tokens.map((line, i) => (
                <div key={i} {...getLineProps({ line })} className={styles.codeLine}>
                  <span className={styles.lineNum}>{i + 1}</span>
                  <span className={styles.lineContent}>
                    {line.map((token, j) => (
                      <span key={j} {...getTokenProps({ token })} />
                    ))}
                  </span>
                </div>
              ))}
            </pre>
          )}
        </Highlight>
      </div>
    </section>
  );
}

function Features() {
  return (
    <section className={styles.features}>
      <div className={styles.featuresInner}>
        {FEATURES.map(({ icon, title, body }) => (
          <div key={title} className={styles.featureCard}>
            <span className={styles.featureIcon}>{icon}</span>
            <h3 className={styles.featureTitle}>{title}</h3>
            <p className={styles.featureBody}>{body}</p>
          </div>
        ))}
      </div>
    </section>
  );
}

function Install() {
  return (
    <section className={styles.install}>
      <div className={styles.installInner}>
        <p className={styles.installLabel}>Get started in seconds</p>
        <div className={styles.installCmd}>
          <code>npm install @jamesholcombe/geo-stream</code>
        </div>
        <p className={styles.installNote}>
          Pre-built native binaries for macOS, Linux, and Windows. No Rust toolchain required.
        </p>
        <Link className={styles.btnPrimary} to="/docs/">
          Read the docs →
        </Link>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title="An embeddable rules engine for location streams"
      description="Drop it into your Node.js process. No server, no network round-trips. Deterministic spatial events with built-in dwell, sequence rules, and speed/heading filters."
      noFooter={false}
    >
      <main className={styles.main}>
        <Hero />
        <CodePreview />
        <Features />
        <Install />
      </main>
    </Layout>
  );
}
