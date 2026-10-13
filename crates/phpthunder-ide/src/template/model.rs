//! The built-in live templates.

use crate::tr;

/// Marks where the caret lands after expansion.
pub const END: &str = "$END$";

/// A live template: an abbreviation and the code it expands to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// What to type before Tab.
    pub abbreviation: &'static str,
    /// Code with [`END`] marking the caret; lines after the first are
    /// indented relative to the line the template is expanded on.
    pub body: &'static str,
}

impl Template {
    /// What the template inserts, in the UI language (shown in the `⌘J` list).
    pub fn description(&self) -> String {
        match self.abbreviation {
            "pubf" => tr!("template-pubf"),
            "prof" => tr!("template-prof"),
            "prif" => tr!("template-prif"),
            "pubsf" => tr!("template-pubsf"),
            "__c" => tr!("template-construct"),
            "fore" => tr!("template-fore"),
            "forek" => tr!("template-forek"),
            "for" => tr!("template-for"),
            "if" => tr!("template-if"),
            "ife" => tr!("template-ife"),
            "try" => tr!("template-try"),
            "fn" => tr!("template-fn"),
            "func" => tr!("template-func"),
            "match" => tr!("template-match"),
            "thr" => tr!("template-thr"),
            "dd" => tr!("template-dd"),
            "dump" => tr!("template-dump"),
            "vd" => tr!("template-vd"),
            "it" => tr!("template-it"),
            "test" => tr!("template-test"),
            "rel" => tr!("template-rel"),
            _ => String::new(),
        }
    }
}

/// Every built-in PHP template, in `⌘J` order.
pub const TEMPLATES: &[Template] = &[
    Template {
        abbreviation: "pubf",
        body: "public function $END$()\n{\n}",
    },
    Template {
        abbreviation: "prof",
        body: "protected function $END$()\n{\n}",
    },
    Template {
        abbreviation: "prif",
        body: "private function $END$()\n{\n}",
    },
    Template {
        abbreviation: "pubsf",
        body: "public static function $END$()\n{\n}",
    },
    Template {
        abbreviation: "__c",
        body: "public function __construct($END$)\n{\n}",
    },
    Template {
        abbreviation: "fore",
        body: "foreach ($END$ as $item) {\n}",
    },
    Template {
        abbreviation: "forek",
        body: "foreach ($END$ as $key => $value) {\n}",
    },
    Template {
        abbreviation: "for",
        body: "for ($i = 0; $i < $END$; $i++) {\n}",
    },
    Template {
        abbreviation: "if",
        body: "if ($END$) {\n}",
    },
    Template {
        abbreviation: "ife",
        body: "if ($END$) {\n} else {\n}",
    },
    Template {
        abbreviation: "try",
        body: "try {\n    $END$\n} catch (\\Throwable $e) {\n}",
    },
    Template {
        abbreviation: "fn",
        body: "fn ($END$) => ",
    },
    Template {
        abbreviation: "func",
        body: "function ($END$) {\n}",
    },
    Template {
        abbreviation: "match",
        body: "match ($END$) {\n    default => null,\n}",
    },
    Template {
        abbreviation: "thr",
        body: "throw new \\RuntimeException('$END$');",
    },
    Template {
        abbreviation: "dd",
        body: "dd($END$);",
    },
    Template {
        abbreviation: "dump",
        body: "dump($END$);",
    },
    Template {
        abbreviation: "vd",
        body: "var_dump($END$);",
    },
    Template {
        abbreviation: "it",
        body: "it('$END$', function () {\n});",
    },
    Template {
        abbreviation: "test",
        body: "public function test$END$(): void\n{\n}",
    },
    Template {
        abbreviation: "rel",
        body: "public function $END$(): \\Illuminate\\Database\\Eloquent\\Relations\\HasMany\n{\n    return $this->hasMany();\n}",
    },
];

/// The template with this abbreviation.
pub fn find_template(abbreviation: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.abbreviation == abbreviation)
}
