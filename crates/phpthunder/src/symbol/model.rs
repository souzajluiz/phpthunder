//! Symbol data: what a declaration is, as stored in the index.

use crate::syntax::Span;

/// What a declaration is; stored on every [`Symbol`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `class`.
    Class,
    /// `interface`.
    Interface,
    /// `trait`.
    Trait,
    /// `enum`.
    Enum,
    /// Top-level `function`.
    Function,
    /// Global `const`.
    Constant,
    /// Method, declared or `@method`.
    Method,
    /// Property: declared, constructor-promoted, or `@property`.
    Property,
    /// Class or interface `const`.
    ClassConstant,
    /// Enum `case`.
    EnumCase,
}

impl Kind {
    /// True for kinds that own members and can be extended, implemented, or used.
    pub fn is_class_like(self) -> bool {
        matches!(
            self,
            Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum
        )
    }

    /// True for kinds declared inside a class; their [`Symbol::fqn`] is the owner's FQN.
    pub fn is_member(self) -> bool {
        matches!(
            self,
            Kind::Method | Kind::Property | Kind::ClassConstant | Kind::EnumCase
        )
    }
}

/// Members are looked up per namespace: methods, properties, and constants
/// (enum cases share the constant namespace, as in PHP).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemberTag {
    /// Methods (case-insensitive names).
    Method,
    /// Properties (case-sensitive names, without `$`).
    Property,
    /// Class constants and enum cases.
    Constant,
}

impl MemberTag {
    /// Lookup namespace of a member kind; `None` for top-level kinds.
    pub fn of(kind: Kind) -> Option<Self> {
        match kind {
            Kind::Method => Some(Self::Method),
            Kind::Property => Some(Self::Property),
            Kind::ClassConstant | Kind::EnumCase => Some(Self::Constant),
            _ => None,
        }
    }
}

/// Index entry for one declaration, built by [`build`].
#[derive(Clone, Debug)]
pub struct Symbol {
    /// What was declared.
    pub kind: Kind,
    /// Declared name (properties without `$`).
    pub name: String,
    /// Fully qualified name for top-level symbols; owner FQN for members.
    pub fqn: String,
    /// Range of the whole declaration.
    pub span: Span,
    /// Range of the name; where go to declaration lands.
    pub name_span: Span,
    /// Byte offset of the name; with the file path it identifies the declaration.
    pub name_start: usize,
    /// Declared `static`; class constants and enum cases count as static.
    pub is_static: bool,
    /// Declaration header, e.g. `public static function create(string $name): self`.
    pub signature: String,
    /// First paragraph of the PHPDoc comment.
    pub doc: String,
    /// Return type (functions/methods) or declared type (properties).
    pub ty: Ty,
    /// Parent class (`extends`) for classes.
    pub extends: Option<String>,
    /// Every supertype: parent class, interfaces, used traits, `@mixin` classes.
    pub supers: Vec<String>,
    /// Declared only in PHPDoc (`@property`, `@method`).
    pub is_virtual: bool,
    /// `@template` parameter names declared on this class, method or function.
    pub templates: Vec<String>,
    /// For classes: generic arguments given to supertypes by `@extends`,
    /// `@implements` and `@use`, as (supertype FQN, arguments).
    pub super_args: Vec<(String, Vec<String>)>,
    /// For methods and functions: `(parameter index, template)` for each
    /// `@param class-string<T>`, so `make(User::class)` binds `T` to `User`.
    pub class_string_params: Vec<(usize, String)>,
}

/// Return/property type sentinel meaning "late static binding": `static` or `$this`.
pub const STATIC_TYPE: &str = "static";

/// Prefix marking a `@template` parameter inside a [`Ty`] (`<T>TValue`);
/// such names are replaced by [`Ty::substitute`] at the use site.
pub const TEMPLATE_PREFIX: &str = "<T>";

/// The template-parameter marker for `name`.
pub fn template_marker(name: &str) -> String {
    format!("{TEMPLATE_PREFIX}{name}")
}

/// The template name if `ty` is a template-parameter marker.
pub fn template_name(ty: &str) -> Option<&str> {
    ty.strip_prefix(TEMPLATE_PREFIX)
}

/// A resolved type: the class an expression evaluates to and, for arrays and
/// generic collections, the class of its elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ty {
    /// Class the value is an instance of; may be [`STATIC_TYPE`] until bound.
    pub class: Option<String>,
    /// Element class of `T[]`, `array<K, T>`, and generic collections.
    pub elem: Option<String>,
    /// Generic arguments (`Collection<int, User>` -> `["", "App\\User"]`);
    /// an empty string stands for a non-class argument.
    pub args: Vec<String>,
}

impl Ty {
    /// A type that is just the class `fqn`, with no element type.
    pub fn class(fqn: impl Into<String>) -> Self {
        Self {
            class: Some(fqn.into()),
            ..Self::default()
        }
    }

    /// True when nothing is known about the type.
    pub fn is_empty(&self) -> bool {
        self.class.is_none() && self.elem.is_none()
    }

    /// Fills whatever `self` lacks from `other` (native types win over docs).
    pub fn or(self, other: Ty) -> Ty {
        let args = if self.args.is_empty() {
            other.args
        } else {
            self.args
        };
        Ty {
            class: self.class.or(other.class),
            elem: self.elem.or(other.elem),
            args,
        }
    }

    /// Replaces template markers using `bindings` (template name ->
    /// concrete class, `""` for non-class); markers left unbound are
    /// dropped so they never leak out as class names.
    pub fn substitute(self, bindings: &std::collections::HashMap<String, String>) -> Ty {
        let bind = |c: Option<String>| match c {
            Some(c) => match template_name(&c) {
                Some(t) => bindings.get(t).filter(|b| !b.is_empty()).cloned(),
                None => Some(c),
            },
            None => None,
        };
        let args = self
            .args
            .into_iter()
            .map(|a| match template_name(&a) {
                Some(t) => bindings.get(t).cloned().unwrap_or_default(),
                None => a,
            })
            .collect();
        Ty {
            class: bind(self.class),
            elem: bind(self.elem),
            args,
        }
    }

    /// Whether any part of the type is still an unbound template marker.
    pub fn has_templates(&self) -> bool {
        self.class
            .as_deref()
            .is_some_and(|c| template_name(c).is_some())
            || self
                .elem
                .as_deref()
                .is_some_and(|c| template_name(c).is_some())
            || self.args.iter().any(|a| template_name(a).is_some())
    }

    /// Replaces [`STATIC_TYPE`] with the class the member was accessed on.
    pub fn bind_static(self, receiver: &str) -> Ty {
        let bind = |c: Option<String>| {
            c.map(|c| {
                if c == STATIC_TYPE {
                    receiver.to_string()
                } else {
                    c
                }
            })
        };
        let args = self
            .args
            .into_iter()
            .map(|a| {
                if a == STATIC_TYPE {
                    receiver.to_string()
                } else {
                    a
                }
            })
            .collect();
        Ty {
            class: bind(self.class),
            elem: bind(self.elem),
            args,
        }
    }
}

/// A member Laravel's Eloquent adds to a model by convention (relation
/// properties, scopes, accessors, attributes from `$fillable`/`$casts`),
/// derived from one declaration by [`eloquent_members_of_method`] and
/// [`eloquent_members_of_property`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConventionMember {
    /// Property or method.
    pub kind: Kind,
    /// Member name (properties without `$`).
    pub name: String,
    /// Property type or method return type.
    pub ty: Ty,
    /// Called statically (scopes: `User::active()`).
    pub is_static: bool,
    /// Byte range in the file to point navigation at (the declaring method
    /// name or the array entry).
    pub start: usize,
    /// End of that range.
    pub end: usize,
}
