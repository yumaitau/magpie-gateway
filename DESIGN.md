# Magpie Gateway design system

## Scene and theme

An administrator reviewing AI activity at a desk in daylight needs a bright, readable working surface. A dark navigation rail and naturalist Australian magpie illustration establish the identity without darkening dense tables and forms.

## Visual direction

Ink and feather: charcoal, soft white and cool grey, with a restrained rust accent drawn from the bird's eye. The login screen uses a detailed naturalist illustration. The application uses a compact bird mark, strong typographic hierarchy, grouped navigation and clean ruled data surfaces.

## Colour

OKLCH tokens in `magpie/crates/magpie-portal/assets/brand.css`. Near-black ink and near-white feather surfaces dominate. Rust is reserved for emphasis and primary sign-in actions; warning, success and danger retain distinct accessible colours with text labels.

## Typography

Local system sans-serif for headings, body, forms and data. Strong weight contrast and deliberate tracking distinguish the wordmark. Tabular numerals for counts and audit data. Fixed rem sizes in the application, readable line lengths and no external fonts.

## Layout and components

The upstream Agentgateway UI in `ui/` is the application foundation. Build it with `VITE_MAGPIE=true` to resolve client and playground URLs through the public shared ingress. The bundled proxy UI remains a fallback for development when no frontend directory is configured. Preserve its native sidebar, light/dark themes, editors, drawers, routing, playgrounds and feature set. Apply local branding assets and add permission-filtered Magpie governance links. Related dashboard tables share a two-column section. Forms use consistent labels, full focus outlines and clearly grouped fieldsets. Tables scroll within their own named region on small screens. At mobile widths navigation becomes a native disclosure.

## Artwork

`magpie/crates/magpie-portal/assets/magpie-portrait-v1.webp`, produced with the built-in image-generation tool. Prompt: a portrait-format contemporary naturalist screenprint of an Australian magpie with black chest, white nape and shoulders, silver bill and rust-red eye, perched on eucalyptus against charcoal; no text or UI. The bird mark is a small code-native SVG. Serve assets from the application itself.

## Motion

Only short colour, border and background transitions for interaction feedback. Respect reduced motion. No decorative page-load animation.
