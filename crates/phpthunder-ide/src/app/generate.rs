//! The `⌘N` Generate menu: constructor, getters and setters, implementing
//! and overriding methods, and PHPUnit test methods and fixtures in the
//! class at the caret.

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use phpthunder::inspection::Edit;
use phpthunder::refactor::{self, ClassOutline};

impl App {
    /// Outline of the class around the caret of the active PHP document.
    fn caret_class(&self) -> Option<ClassOutline> {
        let doc = self.doc().filter(|d| d.is_php())?;
        refactor::class_outline(&doc.text, doc.offset(doc.cursor()))
    }

    /// Opens the Generate menu with the entries that would add something;
    /// shows a notice outside a class or when nothing is missing.
    pub(crate) fn open_generate(&mut self) -> Task<Message> {
        let Some(outline) = self.caret_class() else {
            self.notice = Some(tr!("notice-generate-outside-class"));
            return Task::none();
        };
        let mut entries = Vec::new();
        if refactor::constructor_source(&outline).is_some() {
            entries.push(Generation::Constructor);
        }
        if refactor::accessors_source(&outline, true, false).is_some() {
            entries.push(Generation::Getters);
        }
        if refactor::accessors_source(&outline, false, true).is_some() {
            entries.push(Generation::Setters);
            entries.push(Generation::GettersAndSetters);
        }
        if !self.lang.missing_implementations(&outline.fqn).is_empty() {
            entries.push(Generation::ImplementMethods);
        }
        if !self.lang.overridable_methods(&outline).is_empty() {
            entries.push(Generation::OverrideMethods);
        }
        if self.lang.is_test_class(&outline) {
            entries.push(Generation::TestMethod);
            for fixture in ["setUp", "tearDown"] {
                if refactor::fixture_source(&outline, fixture).is_some() {
                    entries.push(Generation::Fixture(fixture));
                }
            }
        }
        if entries.is_empty() {
            self.notice = Some(tr!("notice-nothing-to-generate"));
            return Task::none();
        }
        let items = entries
            .into_iter()
            .map(|g| Item {
                badge: "+",
                title: g.title(),
                matched: Vec::new(),
                detail: String::new(),
                target: Target::Generate(g),
            })
            .collect();
        self.open_choice(tr!("generate-title"), items)
    }

    /// Runs a Generate menu entry: inserts the code at the end of the class
    /// body and puts the caret on it.
    pub(crate) fn generate(&mut self, generation: Generation) -> Task<Message> {
        let Some(outline) = self.caret_class() else {
            return Task::none();
        };
        let source = match &generation {
            Generation::Constructor => refactor::constructor_source(&outline),
            Generation::Getters => refactor::accessors_source(&outline, true, false),
            Generation::Setters => refactor::accessors_source(&outline, false, true),
            Generation::GettersAndSetters => refactor::accessors_source(&outline, true, true),
            Generation::ImplementMethods => return self.implement_methods(&outline.fqn),
            Generation::OverrideMethods => return self.open_overrides(&outline),
            Generation::Override(name) => self
                .lang
                .overridable_methods(&outline)
                .iter()
                .find(|m| m.name == *name)
                .map(refactor::override_stub),
            Generation::TestMethod => Some(refactor::test_method_source(&outline)),
            Generation::Fixture(name) => refactor::fixture_source(&outline, name),
        };
        let Some(source) = source else {
            return Task::none();
        };
        let at = outline.body_end;
        let caret = at + source.len() - source.trim_start().len();
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.apply_edits(&[Edit {
            path: None,
            start: at,
            end: at,
            text: source,
        }]);
        let position = doc.position_of(caret);
        self.after_change(true);
        self.place_cursor(position)
    }

    /// Second step of "Override methods": pick one parent method.
    fn open_overrides(&mut self, outline: &ClassOutline) -> Task<Message> {
        let items = self
            .lang
            .overridable_methods(outline)
            .into_iter()
            .map(|m| Item {
                badge: "m",
                title: m.name.clone(),
                matched: Vec::new(),
                detail: m.fqn.rsplit('\\').next().unwrap_or(&m.fqn).to_string(),
                target: Target::Generate(Generation::Override(m.name)),
            })
            .collect();
        self.open_choice(tr!("generate-override-title"), items)
    }
}
