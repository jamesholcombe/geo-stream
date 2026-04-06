import type { ReactNode } from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import { Highlight, themes } from 'prism-react-renderer';
import styles from './index.module.css';

const CODE_SNIPPET = `import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()
  .registerZone('warehouse', warehousePolygon)
  // cx = longitude, cy = latitude (same as PointUpdate x / y)
  .registerCircle('depot', -0.12, 51.5, 0.5)
  .defineRule('fast-entry', rule =>
    rule.whenEnters('warehouse').speedAbove(15).emit('alert')
  )

const events = engine.ingest([
  { id: 'driver-1', x: -0.12, y: 51.5, tMs: Date.now() },
])
// [{ kind: 'approach', id: 'driver-1', circle: 'depot', t_ms: ... }]`;

const FEATURES = [
  {
    icon: '⚡',
    title: 'In-process, zero infra',
    body: 'A native Rust module. No server, no database, no network calls. Drop it into any Node.js process.',
  },
  {
    icon: '🎯',
    title: 'Event-first',
    body: 'Entities move. State updates. Events fire. Enter, exit, approach, recede, assignment changed.',
  },
  {
    icon: '🔧',
    title: 'Rules and sequences',
    body: 'Emit custom events when an entity enters a zone at speed. Detect ordered multi-stop routes.',
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
          Turn location streams into
          <span className={styles.heroTitleAccent}> meaningful events</span>
        </h1>
        <p className={styles.heroSubtitle}>
          Entities move through space. geo-stream tracks their state, evaluates
          your rules, and emits typed events — enter, exit, approach, recede —
          synchronously, in-process.
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
      title="Turn location streams into meaningful events"
      description="An embeddable geospatial stream processor. Feed it location updates; receive typed spatial events."
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
