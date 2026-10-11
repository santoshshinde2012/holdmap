# Holdmap identity

The **routed H** combines two offset rails with a bent connection and a terminal node.
It links Holdmap's initial to the process and agent relationships the app makes visible.
The silhouette stays recognizable when the color detail is removed for the menu bar.

[icon.svg](icon.svg) is the vector master. [icon.png](icon.png) is its generated 1024 px
README rendition. Keep the tile and its transparent outer margin intact; the menu-bar
template uses only the `holdmap-mark` group, with a transparent background.

| Element | Color |
| --- | --- |
| App tile | Indigo `#151078` |
| Rails and connection surround | White `#FAFCFF` |
| Route | Cyan `#12D5EE` → blue `#5F78FF` → violet `#8037FA` |

Edit the master, then run from the repository root:

```sh
node apps/desktop/scripts/generate-brand-assets.mjs
```

The script uses the desktop's locked Playwright and Tauri tools. It updates desktop and
website SVG copies, README PNG, favicons, touch icon, native bundle icons and tray assets.
macOS uses the monochrome template so the system controls its appearance in light and
dark menu bars; Windows and Linux use the color tray icon. Platform exports are generated
artifacts, not separate designs. See [CONTRIBUTING](../../../CONTRIBUTING.md) for capture
commands; refresh screenshots, social artwork and all hero videos after a brand change.

The direction was explored with the built-in imagegen tool in new-image mode, with a
transparent background, then reconstructed as a clean SVG for production. The generation
brief was:

> Create one Holdmap application icon: an original routed H with two asymmetric rails,
> a bent connection and an integrated terminal node. Use precise, rounded, flat geometry,
> generous negative space and strong legibility at 16–32 px. Center the mark on an indigo
> rounded square with transparent exterior corners, white rails and cyan/violet route
> accents. No words, robot face, radar ring, separate status badge, tiny ornament, 3D
> rendering, outside shadow or watermark.

Version 0.5.0 includes the generated native icons. Version 0.4.0 installers retain
the original socket identity.
