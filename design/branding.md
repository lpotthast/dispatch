# Branding and Application Icon

Dispatch's application icon represents work routing and coordination. Its capital-`D` silhouette is built from routing lanes converging at a dispatch junction, while the amber square represents a work item moving through the system.

## Current Assets

- `dispatch-server/public/branding/dispatch-icon.png`: 1024px master icon.
- `dispatch-server/public/branding/dispatch-icon-180.png`: Apple touch icon.
- `dispatch-server/public/branding/dispatch-icon-64.png`: compact UI and header icon.
- `dispatch-server/public/branding/favicon-32.png`: browser favicon.

The smaller assets should be derived from the master and checked at their actual display sizes. The route geometry, `D` silhouette, and amber work-item accent must remain legible at 16–18px.

After replacing the master icon, regenerate every smaller asset from the repository root:

```sh
just icons
```

The command runs `scripts/derive-icons.sh`, which accepts an optional master-image path and uses ImageMagick when available or macOS `sips` as a fallback. It validates that the master is square and at least 180px before atomically replacing the 180px, 64px, and 32px variants.

## Generation Prompt

The current icon was generated with the built-in image-generation tool using this prompt:

```text
Use case: logo-brand
Asset type: square application icon and web favicon for Dispatch, a local developer tool that routes software work items to coding agents and tracks workflow lanes.
Primary request: Create one original, memorable geometric icon: a bold capital-D silhouette constructed from two or three clean routing lanes that merge at a central dispatch junction, with one small warm amber rounded-square work item moving through the route. The D must be implied by the geometry, not drawn as ordinary typography.
Style/medium: crisp vector-like flat logo mark rendered as a polished raster icon; minimal; strong silhouette; precise geometry.
Composition/framing: centered on a square canvas; dark ink-navy rounded-square app tile; generous optical padding; thick shapes and simplified detail that remain legible at 16px; balanced negative space.
Color palette: dark ink navy #20242A background, Dispatch cobalt #1D5FA8 and bright off-white #F7FAFC routing paths, one restrained amber #D99024 work-item accent.
Lighting/mood: confident, calm, capable developer tooling; flat colors only.
Text: no text, no letters rendered as text, no wordmark.
Constraints: visually suggest routing, coordination, and forward progress; keep the D silhouette clear; original design only; hard clean edges; no gradients; no shadows; no 3D; no mockup; no border outside the rounded-square tile; no tiny details; no watermark; no unrelated symbols; do not resemble a paper airplane, delivery truck, postal envelope, play button, or existing company logo.
```

## Iteration Guidance

When iterating, retain the semantic concept and palette unless intentionally changing the brand direction. Compare candidates at 1024px, 64px, 32px, and 18px before replacing the current assets. Favor a distinct silhouette and clean route geometry over additional detail.
