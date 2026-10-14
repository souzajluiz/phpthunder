# Inspection diagnostics and quick-fix titles shown in the IDE.
# Keys: <area>-<thing>, lowercase kebab-case.

diagnostic-undefined-class = Undefined class '{ $name }'
diagnostic-undefined-function = Undefined function '{ $name }'
diagnostic-undefined-method = Method '{ $name }' not found in { $class }
diagnostic-undefined-property = Property '${ $name }' not found in { $class }
diagnostic-unused-import = Unused import '{ $name }'
diagnostic-undefined-variable = Undefined variable '{ $name }'

fix-import-class = Import class { $name }
fix-remove-import = Remove unused import
fix-add-method = Add method '{ $name }()' to { $class }
fix-add-property = Add property '${ $name }' to { $class }

diagnostic-unused-variable = Variable '{ $name }' is assigned but never used
diagnostic-missing-return = Missing return statement: '{ $ty }' expected
diagnostic-return-mismatch = Return type mismatch: '{ $expected }' expected, { $found } returned
fix-remove-assignment = Remove unused variable (keep the value)
diagnostic-unknown-laravel-key = { $kind ->
    [Route] Unknown route
    [View] Unknown view
    [Config] Unknown config key
    [Translation] Unknown translation key
    [Guard] Unknown guard
    [Disk] Unknown disk
    [CacheStore] Unknown cache store
    [QueueConnection] Unknown queue connection
    [LogChannel] Unknown log channel
    [DbConnection] Unknown database connection
    [Mailer] Unknown mailer
    [InertiaPage] Unknown Inertia page
    [Livewire] Unknown Livewire component
   *[other] Unknown key
} '{ $name }'
diagnostic-simplify-order-by = orderBy(…, 'desc') can be orderByDesc(…)
fix-use-order-by-desc = Use orderByDesc()
fix-blade-echo-raw = Output unescaped ({"{"}!! !!{"}"})
fix-blade-echo-escaped = Escape output ({"{"}{"{"} {"}"}{"}"})
fix-rules-to-array = Convert rules to an array
fix-rules-to-string = Convert rules to a string

# Syntax-only inspections.
diagnostic-unreachable-code = Unreachable statement
diagnostic-duplicate-array-key = Duplicate array key '{ $key }'; the later value wins
diagnostic-assignment-in-condition = Assignment in condition; did you mean ==?
diagnostic-loose-null-comparison = Loose comparison with null also matches 0, '' and []
diagnostic-empty-catch = Empty catch block hides errors
diagnostic-self-assignment = Value is assigned to itself
diagnostic-unused-private-member = Unused private member { $name }
fix-use-strict-comparison = Use strict comparison
fix-remove-statement = Remove statement
diagnostic-empty-body = Empty statement body: the next statement always runs
diagnostic-duplicate-case = Duplicate case { $value } never matches
diagnostic-duplicate-condition = Condition repeats an earlier branch; this branch never runs
diagnostic-identical-branches = Both branches of the ternary are the same
diagnostic-same-operands = Both sides of the operation are the same
diagnostic-unused-closure-use = { $name } is imported by use but never read
diagnostic-duplicate-member = { $name } is declared twice
diagnostic-abstract-in-concrete-class = Abstract method in a class that is not abstract
diagnostic-this-in-static = $this is not available in a static method
diagnostic-redundant-else = Redundant else: the if branch always leaves
fix-remove-redundant-else = Remove redundant else

## Rule-based inspections (`Rule::slug`)
diagnostic-missing-break = Case falls through to the next one; add break
diagnostic-duplicate-match-condition = Duplicate match condition { $detail } never matches
diagnostic-optional-before-required = Optional parameter { $detail } before a required one
diagnostic-division-by-zero = Division by zero
diagnostic-foreach-overwrites-array = { $detail } is both the iterated array and a loop variable
diagnostic-foreach-variable-conflict = { $detail } overwrites the outer foreach variable
diagnostic-output-function-result-used = { $detail }() prints and returns null unless its second argument is true
fix-output-function-result-used = Pass true to return the output
diagnostic-rand-arguments-reversed = Minimum is greater than maximum
fix-rand-arguments-reversed = Swap arguments
diagnostic-expression-result-unused = Expression result is not used
diagnostic-continue-in-switch = continue inside switch acts like break; use continue 2
fix-continue-in-switch = Replace with continue 2
diagnostic-exception-immediately-rethrown = Exception is caught only to be rethrown
diagnostic-nested-ternary = Nested ternary without parentheses is a fatal error since PHP 8
diagnostic-dollar-brace-interpolation = ${ "{" }var{ "}" } string interpolation is deprecated; use { "{" }$var{ "}" }
fix-dollar-brace-interpolation = Use { "{" }$var{ "}" }
diagnostic-implicitly-nullable-parameter = Implicitly nullable parameter { $detail } is deprecated; make the type nullable
fix-implicitly-nullable-parameter = Make the type nullable
diagnostic-magic-method-not-public = Magic method { $detail } must be public
fix-magic-method-not-public = Make public
diagnostic-magic-method-static = Magic method { $detail } has the wrong static modifier
diagnostic-inconsistent-return = Function returns a value on some paths only
diagnostic-search-result-as-boolean = { $detail }() can return 0, which reads as false; compare with !== false
diagnostic-format-arguments-mismatch = Format expects { $detail } arguments
diagnostic-redundant-promoted-assignment = Promoted property is already assigned
fix-redundant-promoted-assignment = Remove assignment
diagnostic-value-immediately-rewritten = { $detail } is overwritten before it is read
fix-value-immediately-rewritten = Remove first assignment
diagnostic-removed-cast = The { $detail } cast was removed in PHP 8
diagnostic-ternary-to-short = Ternary can be shortened to ?:
fix-ternary-to-short = Replace with ?:
diagnostic-ternary-to-condition = Ternary can be replaced with its condition
fix-ternary-to-condition = Replace with condition
diagnostic-isset-to-coalesce = isset() ternary can be replaced with ??
fix-isset-to-coalesce = Replace with ??
diagnostic-nullsafe-can-be-used = Null check can be replaced with ?->
fix-nullsafe-can-be-used = Replace with ?->
diagnostic-combine-isset = isset() calls can be combined
fix-combine-isset = Combine isset() calls
diagnostic-boolean-comparison = Comparison with a boolean can be simplified
fix-boolean-comparison = Simplify
diagnostic-array-push-single = array_push() with one value can be $array[] =
fix-array-push-single = Replace with $array[] =
diagnostic-pow-to-operator = pow() can be replaced with **
fix-pow-to-operator = Replace with **
diagnostic-long-array-syntax = array() can be replaced with []
fix-long-array-syntax = Replace with []
diagnostic-redundant-closing-tag = Redundant closing tag
fix-redundant-closing-tag = Remove closing tag
diagnostic-closure-to-arrow-function = Closure can be an arrow function
fix-closure-to-arrow-function = Convert to arrow function
diagnostic-get-class-to-class-constant = get_class() can be replaced with ::class
fix-get-class-to-class-constant = Replace with ::class
diagnostic-str-function-can-be-used = Can be replaced with { $detail }()
fix-str-function-can-be-used = Replace
diagnostic-unnecessary-local-variable = Local variable { $detail } is returned right away
fix-unnecessary-local-variable = Inline variable
diagnostic-silence-operator = Error silencing with @
diagnostic-unnecessary-bool-cast = Unnecessary (bool) cast
fix-unnecessary-bool-cast = Remove cast
diagnostic-redundant-break-argument = Redundant argument 1
fix-redundant-break-argument = Remove argument
diagnostic-in-array-single-element = in_array() with one element can be a comparison
fix-in-array-single-element = Replace with comparison
diagnostic-array-append-using-count = $array[count($array)] can be $array[]
fix-array-append-using-count = Replace with $array[]
diagnostic-nested-dirname = Nested dirname() calls can use the levels argument
fix-nested-dirname = Use the levels argument
diagnostic-nested-min-max = Nested { $detail }() calls can be merged
fix-nested-min-max = Merge calls
diagnostic-match-only-default = match has only a default arm
diagnostic-unnecessary-semicolon = Unnecessary semicolon
fix-unnecessary-semicolon = Remove semicolon
diagnostic-unused-parameter = Unused parameter { $detail }
diagnostic-else-if = else if should be written elseif
fix-else-if = Replace with elseif
diagnostic-var-keyword = var should be replaced with public
fix-var-keyword = Replace with public
diagnostic-missing-visibility = { $detail } has no visibility
fix-missing-visibility = Add public
diagnostic-modifier-order = Modifiers out of order
fix-modifier-order = Reorder modifiers
diagnostic-new-without-parentheses = new without parentheses
fix-new-without-parentheses = Add parentheses
diagnostic-long-type-cast = Use the short cast ({ $detail })
fix-long-type-cast = Use short cast
diagnostic-multiple-classes-in-file = File declares more than one class
diagnostic-phpdoc-unknown-parameter = @param { $detail } names no parameter
diagnostic-assert-equals-literal = Can be { $detail }()
fix-assert-equals-literal = Replace assertion
diagnostic-assert-count = Can be assertCount()
fix-assert-count = Replace assertion
diagnostic-assert-array-has-key = Can be { $detail }()
fix-assert-array-has-key = Replace assertion
diagnostic-assert-empty = Can be { $detail }()
fix-assert-empty = Replace assertion
diagnostic-assert-contains = Can be { $detail }()
fix-assert-contains = Replace assertion
diagnostic-assert-instance-of = Can be { $detail }()
fix-assert-instance-of = Replace assertion
diagnostic-assert-arguments-order = Expected value should come first
fix-assert-arguments-order = Swap arguments
diagnostic-missing-implementation = Class must implement { $detail }
fix-missing-implementation = Implement methods
diagnostic-extends-final-class = Cannot extend final class { $detail }
diagnostic-overrides-final-method = Cannot override final method { $detail }
diagnostic-wrong-supertype-kind = { $detail } cannot be used here
diagnostic-instantiate-abstract = Cannot instantiate { $detail }
diagnostic-weaker-access = Visibility must be { $detail } or weaker, as in the parent
fix-weaker-access = Use the parent's visibility
diagnostic-static-mismatch = Override of { $detail } changes static
diagnostic-signature-mismatch = Declaration must be compatible with { $detail }
diagnostic-too-few-arguments = Expects at least { $detail } arguments
diagnostic-too-many-arguments = Expects at most { $detail } arguments
diagnostic-inaccessible-member = Member { $detail } is not accessible here
diagnostic-static-call-to-instance-method = Non-static method { $detail } called statically
diagnostic-undefined-class-constant = Undefined class constant { $detail }
diagnostic-void-result-used = { $detail } returns void; its result is always null
diagnostic-catch-order = Already caught by the earlier catch of { $detail }
diagnostic-enum-match-not-exhaustive = match misses enum cases: { $detail }
diagnostic-missing-parent-constructor-call = Missing parent constructor call
diagnostic-readonly-modified = Readonly property { $detail } cannot be modified here
diagnostic-undefined-data-provider = Data provider { $detail } not found

## Context actions (`intention::Action::slug`)
intention-to-double-quotes = Convert to double-quoted string
intention-to-single-quotes = Convert to single-quoted string
intention-concat-to-interpolation = Replace concatenation with interpolation
intention-flip-binary = Flip operands
intention-invert-if = Invert if condition
intention-merge-nested-if = Merge nested if
intention-split-and-condition = Split into nested if
intention-add-braces = Add braces
intention-remove-braces = Remove braces
intention-if-to-ternary = Replace if with ternary
intention-ternary-to-if = Replace ternary with if
intention-switch-to-match = Replace switch with match
intention-arrow-to-closure = Convert to closure
intention-add-void-return-type = Add void return type
intention-promote-property = Promote to constructor property
intention-add-strict-types = Add strict_types declaration
intention-import-class = Import class
intention-add-override = Add #[\Override]
intention-make-final = Make class final
intention-split-array = Put elements on separate lines
intention-join-array = Put elements on one line
intention-strict-comparison = Use strict comparison
intention-to-compound-assignment = Use compound assignment
intention-expand-compound-assignment = Expand compound assignment
intention-to-increment = Use increment
intention-de-morgan = Apply De Morgan's law
intention-generate-phpdoc = Generate PHPDoc
intention-add-property-hooks = Add property hooks
diagnostic-deprecated-usage = { $detail } is deprecated
diagnostic-redundant-method-override = Method only calls its parent
fix-redundant-method-override = Remove method
diagnostic-unnecessary-fully-qualified-name = { $detail } can be written by its short name
fix-unnecessary-fully-qualified-name = Use short name
diagnostic-leading-backslash-in-use = Leading backslash in use is unnecessary
fix-leading-backslash-in-use = Remove backslash
diagnostic-loop-never-iterates = Loop never iterates more than once
diagnostic-condition-already-checked = Condition is always true here
diagnostic-duplicate-case-body = Same statements as case { $detail }
diagnostic-duplicate-arm-body = Same result as the arm for { $detail }
diagnostic-unnecessary-stop-statement = Unnecessary { $detail }
fix-unnecessary-stop-statement = Remove statement
diagnostic-duplicate-class-declaration = Class is also declared in { $detail }
