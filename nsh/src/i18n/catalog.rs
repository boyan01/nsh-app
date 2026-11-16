//! Typed ARB lookup. JSON is parsed only by build.rs, never at runtime.
include!(concat!(env!("OUT_DIR"), "/i18n.rs"));

pub(super) fn format(locale: &str, key: Key, arguments: &[(&str, &str)]) -> String {
    let template = lookup(locale, key);
    let mut output = String::with_capacity(template.len());
    let mut copied = 0;
    // Replace names only in the original template. Argument values stay verbatim.
    for (start, _) in template.match_indices('{') {
        if start < copied {
            continue;
        }
        if let Some((name, value)) = arguments.iter().find(|(name, _)| {
            template[start + 1..].starts_with(name)
                && template.as_bytes().get(start + name.len() + 1) == Some(&b'}')
        }) {
            output.push_str(&template[copied..start]);
            output.push_str(value);
            copied = start + name.len() + 2;
        }
    }
    output.push_str(&template[copied..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arb_keys_select_their_own_messages_without_english_matching() {
        // Both keys describe a system preference, with different English copy.
        assert_eq!(lookup("en", Key::FollowSystem), "Follow System");
        assert_eq!(lookup("en", Key::NshSystemLanguage), "System language");
        assert_eq!(lookup("zh-CN", Key::Today), "今天");
        assert_eq!(lookup("zh-TW", Key::NshPosts), "文章");
        assert_eq!(lookup("zh-HK", Key::NshPosts), "文章");
    }

    #[test]
    fn missing_locale_messages_and_unsupported_locales_fall_back_to_english() {
        // NSH's Posts label has no Spanish entry; the generated table fills it.
        assert_eq!(lookup("es", Key::NshPosts), "Posts");
        assert_eq!(lookup("unsupported", Key::NshPosts), "Posts");
        assert_eq!(lookup("system", Key::NshSystemLanguage), "System language");
    }

    #[test]
    fn positional_and_named_placeholders_follow_localized_word_order() {
        for (locale, expected) in [
            ("en", "12 PARTICIPANTS"),
            ("id", "Anggota: 12"),
            ("ms", "Ahli: 12"),
        ] {
            assert_eq!(
                format(locale, Key::ParticipantsCount, &[("arg0", "12")]),
                expected,
                "{locale}"
            );
        }
        assert_eq!(
            format(
                "zh-CN",
                Key::NshSenderAddedParticipant,
                &[("sender", "Alice"), ("participant", "Bob")]
            ),
            "Alice添加了Bob"
        );
        assert_eq!(
            format("es", Key::NshSelectedContactsCount, &[("count", "3")]),
            "Selected contacts: 3"
        );
    }

    #[test]
    fn placeholder_values_remain_verbatim_and_unknown_names_remain_visible() {
        assert_eq!(
            format(
                "en",
                Key::NshSenderAddedParticipant,
                &[("sender", "{participant}"), ("participant", "Bob")]
            ),
            "{participant} added Bob"
        );
        assert_eq!(
            format("en", Key::NshSenderAddedParticipant, &[("send", "Alice")]),
            "{sender} added {participant}"
        );
    }

    #[test]
    fn explicit_arb_templates_format_all_placeholders_in_every_supported_locale() {
        for key in KEYS {
            let english = lookup("en", *key);
            // Original Flutter ICU messages are preserved, not interpreted here.
            if english.contains(", plural,") || english.contains(", select,") {
                continue;
            }
            let names: Vec<_> = english
                .split('{')
                .skip(1)
                .filter_map(|tail| tail.split_once('}').map(|(name, _)| name))
                .collect();
            let arguments: Vec<_> = names.iter().map(|name| (*name, "VALUE")).collect();
            for language in crate::i18n::language::Language::SUPPORTED {
                let output = format(language.value(), *key, &arguments);
                assert!(
                    !output.contains(['{', '}']),
                    "{language:?} {key:?}: {output}"
                );
                if !arguments.is_empty() {
                    assert!(output.contains("VALUE"), "{language:?} {key:?}: {output}");
                }
            }
        }
    }
}
