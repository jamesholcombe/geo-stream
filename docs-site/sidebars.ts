import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  mainSidebar: [
    {
      type: 'category',
      label: 'Getting Started',
      collapsible: false,
      items: ['intro', 'installation'],
    },
    {
      type: 'category',
      label: 'Concepts',
      collapsible: true,
      collapsed: false,
      items: ['zone-types', 'ingest', 'dwell'],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsible: true,
      collapsed: false,
      items: ['events-reference'],
    },
    {
      type: 'category',
      label: 'Examples',
      collapsible: true,
      collapsed: false,
      items: [
        'examples/basic-zone',
        'examples/multi-zone',
        'examples/dwell-debounce',
      ],
    },
  ],
};

export default sidebars;
