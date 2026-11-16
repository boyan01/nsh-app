# Bundled assets

SVG files under this directory are discovered recursively by `nsh/build.rs`.
Use the generated `crate::assets::svg` constants instead of embedding SVG markup
or writing asset paths in presentation code. Names use the asset-relative path
without `.svg`, with separators converted to underscores and letters uppercased:
`menu/forward.svg` becomes `svg::MENU_FORWARD` and `icons/spinner.svg` becomes
`svg::ICONS_SPINNER`. Colliding names fail the build.

The `icons/` directory contains the desktop stroke icons, busy indicator and
location pin. Their original geometry, view boxes, stroke widths and transforms
are preserved; tinted shapes use `currentColor`. Unit tests parse every generated
asset as SVG. Raster images and packaged sound/license resources retain their
existing loading paths.

Localization authoring and fallback rules are described in [l10n/README.md](l10n/README.md).
