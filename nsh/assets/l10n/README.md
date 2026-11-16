# ARB localization resources

These nine ARB files are the single localization source for NSH. The original
Flutter entries were copied from `MixinNetwork/flutter-app` at
`7fa2d1ba5ab65f8a7b4418e685b4148ff81c09e9`; their keys and values remain intact.
Additional desktop copy uses `nsh...` keys in the same files, including regional
Chinese translations and the former supplemental translations.

`nsh/build.rs` reads `intl_*.arb` and generates `i18n::Key` and static locale
lookup tables in Cargo's `OUT_DIR`. Each variant is its ARB key with the first
letter capitalized (`participantsCount` becomes `Key::ParticipantsCount`).
Call sites use `i18n.t(Key::Today)` or
`i18n.format(Key::ParticipantsCount, &[("arg0", &count.to_string())])`.
There is no runtime ARB parsing, English reverse lookup, alias table, or separate
supplemental catalog. New copy must have an English entry; add simplified,
Taiwan and Hong Kong Chinese entries alongside it. Other missing locale entries
fall back to English when the tables are generated.

The picker exposes all nine locales plus the system preference. The Indonesian
catalog keeps the legacy filename `intl_in.arb`; its public tag is `id` and
persisted `in` and `id` are both accepted. Simplified Chinese accepts `zh-CN`,
`zh` and `zh-Hans`; Taiwan / Hant and Hong Kong / Macao resolve to their regional
catalogs. Unsupported system locales resolve to English. Traditional Chinese
never implicitly falls back to simplified Chinese.

Placeholder formatting substitutes explicit `{arg0}` or named `{sender}` tokens
in the original template. Values are appended verbatim and are never expanded
again. Unknown or missing placeholders remain visible. Add ARB placeholder
metadata for new templates. Original Flutter ICU messages remain intact; this
formatter does not interpret ICU plural/select syntax. NSH duration copy selects
its singular or plural key before formatting, and participant counts use the
original `participantsCount` key.

Language changes update the presentation signal immediately and persist through
`SettingsClient` under `desktop_ui_language`. A failed write restores the previous
selection; choosing System clears the durable override. Static text still needs
a reactive closure or a presentation subtree keyed by `I18n::locale_tag()`.
Account services, navigation, draft state and session owners retain their
existing lifetimes. Embedded bot contexts and native bot labels consume the same
locale selection.

Catalog and language tests cover direct lookup, English fallback, positional and
named formatting, argument values containing braces and locale resolution.
Translation coverage and compilation do not establish native-speaker approval,
text expansion, live language-switching or visual layout acceptance.
