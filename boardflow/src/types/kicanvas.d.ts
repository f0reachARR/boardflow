import type { DetailedHTMLProps, HTMLAttributes } from 'react';

interface KiCanvasEmbedAttributes {
  src?: string;
  controls?: 'none' | 'basic' | 'full';
  controlslist?: string;
  theme?: string;
  zoom?: string;
}

interface KiCanvasSourceAttributes {
  src?: string;
  type?: 'schematic' | 'board' | 'project' | 'worksheet';
  name?: string;
}

declare module 'react' {
  namespace JSX {
    interface IntrinsicElements {
      'kicanvas-embed': DetailedHTMLProps<
        HTMLAttributes<HTMLElement> & KiCanvasEmbedAttributes,
        HTMLElement
      >;
      'kicanvas-source': DetailedHTMLProps<
        HTMLAttributes<HTMLElement> & KiCanvasSourceAttributes,
        HTMLElement
      >;
    }
  }
}
