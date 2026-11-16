use mixin_desktop_api::SettingsClient;
use nsh_base::SelectItem;
use nsh_core::prelude::*;

use crate::{
    runtime,
    ui::{self, Typography as _},
};

const LANGUAGE_SETTING: &str = "desktop_ui_language";

pub(crate) mod catalog;
mod language;

pub(crate) use catalog::Key;
use language::Language;

#[derive(Clone, Copy)]
pub(crate) struct I18n {
    owner: Owner,
    language: Signal<Language>,
    settings: Stored<Option<SettingsClient>>,
    saving: Signal<bool>,
    failed: Signal<bool>,
}

impl I18n {
    pub(crate) fn new() -> Self {
        Self {
            owner: Owner::current(),
            language: signal(Language::System),
            settings: stored(None),
            saving: signal(false),
            failed: signal(false),
        }
    }
    #[cfg(test)]
    pub(crate) fn for_test(language: &str) -> Self {
        let i18n = Self::new();
        i18n.language.set(Language::parse(language));
        i18n
    }

    pub(crate) async fn load(&self, settings: SettingsClient) {
        let read = settings.clone();
        match runtime::run(async move { read.setting(LANGUAGE_SETTING.into()).await }).await {
            Ok(value) => self
                .language
                .set(Language::parse(value.as_deref().unwrap_or("system"))),
            Err(_) => self.failed.set(true),
        }
        self.settings.replace(Some(settings));
    }
    fn resolved(self) -> Language {
        match self.language.get() {
            Language::System => Language::system(locale().as_str()),
            language => language,
        }
    }
    /// Reactive BCP 47 locale for presentation and embedded bot contexts.
    pub(crate) fn locale_tag(self) -> &'static str {
        self.resolved().value()
    }
    pub(crate) fn chinese(self) -> bool {
        self.locale_tag().starts_with("zh")
    }
    pub(crate) fn t(self, key: Key) -> &'static str {
        catalog::lookup(self.locale_tag(), key)
    }
    pub(crate) fn format(self, key: Key, arguments: &[(&str, &str)]) -> String {
        catalog::format(self.locale_tag(), key, arguments)
    }
    fn choose(self, language: Language) {
        if self.saving.get_untracked() || language == self.language.get_untracked() {
            return;
        }
        let Some(settings) = self.settings.with(|value| value.clone()) else {
            return;
        };
        let previous = self.language.get_untracked();
        self.language.set(language);
        self.saving.set(true);
        self.failed.set(false);
        self.owner.run(|| {
            spawn(async move {
                let value = (language != Language::System).then(|| language.value().to_owned());
                if runtime::commit(async move {
                    settings.set_setting(LANGUAGE_SETTING.into(), value).await
                })
                .await
                .is_err()
                {
                    self.language.set(previous);
                    self.failed.set(true);
                }
                self.saving.set(false);
            })
        });
    }
    pub(crate) fn picker(self) -> impl IntoElement {
        self.language_select(
            ui::command()
                .child(ui::icon(ui::Icon::Globe))
                .child(text(move || self.language_title()).text_role(ui::TextRole::Body))
                .child(ui::icon(ui::Icon::ChevronDown)),
        )
    }
    /// The picker for a settings row, matching the row's other selects.
    pub(crate) fn setting_picker(self) -> impl IntoElement {
        self.language_select(ui::select_trigger(move || self.language_title()))
    }
    fn language_title(self) -> &'static str {
        match self.language.get() {
            Language::System => self.t(Key::NshSystemLanguage),
            language => language.title(),
        }
    }
    fn language_select(self, trigger: impl IntoElement + 'static) -> impl IntoElement {
        view()
            .gap_1()
            .child(
                ui::select()
                    .items(move || {
                        let mut items =
                            vec![SelectItem::new("system", self.t(Key::NshSystemLanguage))];
                        items.extend(
                            Language::SUPPORTED.into_iter().map(|language| {
                                SelectItem::new(language.value(), language.title())
                            }),
                        );
                        items
                    })
                    .value(move || Some(SharedString::from(self.language.get().value())))
                    .disabled(move || self.saving.get())
                    .accessibility_label(move || Some(self.t(Key::NshLanguage).into()))
                    .on_change(move |next: &Option<SharedString>| {
                        if let Some(value) = next {
                            self.choose(Language::parse(value.as_str()));
                        }
                    })
                    .offset(ui::space_xs())
                    .trigger(trigger),
            )
            .child(when(
                move || self.failed.get(),
                move || {
                    text(move || self.t(Key::NshCouldnTSaveTheLanguageTryAgain))
                        .text_role(ui::TextRole::Caption)
                        .text_color(ui::danger)
                },
            ))
    }
}

/// Use inside reactive text bindings, or a locale-keyed presentation subtree.
pub(crate) fn t(key: Key) -> &'static str {
    current().t(key)
}

pub(crate) fn current() -> I18n {
    use_context::<I18n>().expect("App provides translations")
}
