# NSH localization coverage

All fixed English/Chinese translation pairs and former supplemental messages
have ARB keys. Rust translation call sites consume the generated `i18n::Key`
values directly, including menu options, preview labels, system-message
arguments, disappearing-message units and native bot chrome.

The original Flutter keys and translations are preserved. NSH-only keys live in
the ARB catalogs and have English, simplified Chinese, Taiwan Chinese and Hong
Kong Chinese values. Existing additional locale translations were retained;
missing entries in Spanish, Indonesian, Japanese, Malay and Russian use English.
New regional translations still require native-speaker review.

This coverage excludes user content, server-provided text, fixture messages,
brand names, binary units, protocol identifiers and English-only accessibility
labels. Date/time formatting is owned by the presentation layer. The original
Flutter ICU plural/select entries are preserved but are not interpreted by the
simple explicit-placeholder formatter; NSH uses separate keys where needed.

Source and unit-test coverage do not establish native font measurement, text
expansion, visual layout, live language switching or Windows/Linux acceptance.
