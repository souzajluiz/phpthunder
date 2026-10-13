# Notices, menu entries and tool messages raised by IDE actions.
# Keys: <area>-<thing>, lowercase kebab-case.

notice-no-intentions = No actions here
notice-no-suggestions = No suggestions
notice-no-declaration = No declaration found
notice-nothing-to-search = Nothing to search: place the cursor on a symbol
notice-open-failed = Could not open { $path }: { $error }
preview-binary = Binary file ({ $size }), not shown in the editor
notice-read-failed = Could not read { $path }
notice-write-failed = Could not write { $path }: { $error }
notice-save-failed = Could not save { $file }: { $error }
notice-save-failed-many = Could not save { $files }
notice-invalid-pattern = Invalid pattern: { $error }
notice-replaced = Replaced { $count ->
    [one] 1 occurrence
   *[other] { $count } occurrences
}

notice-caret-class-or-method = Place the caret on a class or method
notice-caret-in-class = Place the caret on or inside a class
notice-caret-on-name = Place the caret on a name to rename
notice-no-implementations = No implementations of { $name } found
notice-no-parameter-info = No parameter info here
implementations-title = Implementations of { $name }
hierarchy-title = Type hierarchy of { $name }
hierarchy-not-indexed = not indexed

notice-renamed = Renamed { $occurrences ->
    [one] 1 occurrence
   *[other] { $occurrences } occurrences
} in { $files ->
    [one] 1 file
   *[other] { $files } files
}
notice-rename-exists = { $path } already exists; file not renamed
notice-rename-failed = Could not rename { $path }: { $error }

intention-implement-methods = Implement methods
intention-optimize-imports = Optimize imports
notice-implemented = Implemented { $count ->
    [one] 1 method
   *[other] { $count } methods
}
notice-imports-optimized = Imports are already optimized

notice-formatted = Formatted with { $formatter }
notice-whitespace-trimmed = No project formatter: trailing whitespace removed
notice-no-composer = No composer.json in this project

composer-title = Composer
composer-install = Install dependencies from composer.lock
composer-update = Update dependencies to their latest allowed versions
composer-dump-autoload = Regenerate the autoloader
composer-outdated = List outdated direct dependencies
composer-validate = Check composer.json
composer-script = script
composer-steps = { $count ->
    [one] 1 step
   *[other] { $count } steps
}

error-run-php = cannot run php: { $error }
error-no-test-runner = No test runner found (install pestphp/pest or phpunit/phpunit)
error-formatter-failed = { $formatter } failed: { $error }
error-phpstan-missing = PHPStan is not installed
error-phpstan-no-report = PHPStan returned no report
error-terminal-open = cannot open a terminal: { $error }
error-terminal-shell = cannot start the shell: { $error }
error-indexing = indexing failed: { $error }
error-rename = rename failed: { $error }
error-invalid-pattern = invalid pattern

generate-title = Generate
generate-constructor = Constructor
generate-getters = Getters
generate-setters = Setters
generate-getters-setters = Getters and setters
generate-override = Override methods…
generate-override-title = Override method
notice-generate-outside-class = Place the caret inside a class to generate code
notice-nothing-to-generate = Nothing to generate here
templates-title = Insert live template

error-git-start = cannot run git: { $error }

git-not-repository = This project is not a Git repository
git-detached = detached HEAD
git-refresh = Refresh
git-pull = Pull (fast-forward)
git-push = Push
git-discard = Discard changes
git-discard-confirm = Discard?
git-clean = No changes
git-stage-all = Stage all
git-unstage-all = Unstage all
git-message-placeholder = Commit message
git-commit = Commit
git-commit-push = Commit and push
git-select-file = Select a file to see its changes
git-staging = Staging…
git-committing = Committing…
git-pushing = Pushing…
git-pulling = Pulling…
git-discarding = Discarding…
git-switching = Switching branch…
git-failed = Git: { $error }
git-empty-message = Write a commit message first
git-nothing-staged = Stage at least one file to commit
git-committed = Committed: { $summary }
git-pushed = Pushed
git-pulled = Pulled
git-discarded = Changes discarded
git-switched = Branch switched
git-branches-title = Branches
git-branch-current = current
git-branch-remote = remote
git-create-branch = create branch from HEAD

debug-test = Debug test
debug-again = Debug again
debug-hint = Set breakpoints by clicking line numbers, then debug a test with ⌃⇧D
debug-starting = waiting for Xdebug…
debug-running = running
debug-paused = paused
debug-ended = ended
debug-resume = Resume
debug-step-over = Step over
debug-step-into = Step into
debug-step-out = Step out
debug-stop = Stop
debug-eval-placeholder = Evaluate expression ($user->name)
debug-finished = Debug session ended
debug-failed = Debugger: { $error }
debug-no-xdebug = Xdebug is not installed for this PHP (install it, or set PHPTHUNDER_XDEBUG to xdebug.so)
debug-no-connection = Xdebug did not connect (is xdebug.mode allowed and the port free?)
debug-connection-closed = Xdebug closed the connection
debug-no-value = no value

notice-changed-on-disk = Changed on disk while you have unsaved edits: { $files }

file-new-file = New file…
tree-hide-dot-folders = Hide dot folders
tree-show-dot-folders = Show dot folders
tree-hide-dependencies = Hide node_modules and vendor
tree-show-dependencies = Show node_modules and vendor
file-new-folder = New folder…
file-new-class = New PHP class…
file-rename = Rename or move…
file-delete = Move to Trash
file-reveal = Reveal in Finder
file-copy-path = Copy relative path
file-new-file-in = New file in { $dir }
file-new-folder-in = New folder in { $dir }
file-new-class-in = New PHP class in { $dir }
file-rename-title = Rename or move { $path }
file-prompt-hints = ↵ confirm   esc cancel
file-deleted = Moved to Trash: { $path }
file-op-failed = File operation failed: { $error }
file-bad-name = Type a name inside the project
file-exists = { $path } already exists

refactor-extract-variable = Extract variable
refactor-extract-method = Extract method
refactor-inline-variable = Inline variable
refactor-change-signature = Change signature
refactor-move-class = Move class to namespace
refactor-extract-variable-help = Name of the new variable; identical occurrences in the statement are replaced too.
refactor-extract-method-help = Name of the new private method; variables defined before become parameters.
refactor-change-signature-help = Edit the parameter list: reorder, rename types or add parameters (calls pass a new parameter's default, else null).
refactor-move-class-help = New namespace; the file moves to its PSR-4 folder and imports elsewhere follow.
refactor-select-first = Select an expression or statements first
refactor-no-callable = Place the caret on a function or method

replace-in-path = Replace all
replace-in-path-confirm = Replace { $count } matches? Press again
replace-in-path-done = Replaced { $count ->
    [one] 1 match
   *[other] { $count } matches
} in { $files ->
    [one] 1 file
   *[other] { $files } files
}
replace-in-path-failed = Could not write: { $files }

# Command names (Find Action palette, ⇧⌘A).
cmd-save = Save
cmd-close-tab = Close tab
cmd-toggle-sidebar = Show or hide project tree
cmd-next-tab = Next tab
cmd-prev-tab = Previous tab
cmd-undo = Undo
cmd-redo = Redo
cmd-toggle-comment = Comment with line comment
cmd-duplicate = Duplicate line or selection
cmd-delete-line = Delete line
cmd-move-line-up = Move line up
cmd-move-line-down = Move line down
cmd-complete = Code completion
cmd-goto-declaration = Go to declaration
cmd-find-usages = Find usages
cmd-goto-line = Go to line
cmd-goto-file = Go to file
cmd-goto-class = Go to class
cmd-goto-symbol = Go to symbol
cmd-recent-files = Recent files
cmd-search-everywhere = Search everywhere
cmd-back = Back
cmd-forward = Forward
cmd-find = Find
cmd-replace = Replace
cmd-find-next = Find next
cmd-find-previous = Find previous
cmd-find-in-path = Find in files
cmd-quick-fix = Show intentions and quick fixes
cmd-parameter-info = Parameter info
cmd-rename = Rename
cmd-optimize-imports = Optimize imports
cmd-goto-implementation = Go to implementation
cmd-type-hierarchy = Type hierarchy
cmd-file-structure = File structure
cmd-problems = Problems
cmd-toggle-terminal = Terminal
cmd-toggle-claude = Claude
cmd-run-tests = Run tests
cmd-reformat = Reformat code
cmd-composer = Composer
cmd-generate = Generate
cmd-insert-template = Insert live template
cmd-commit = Commit
cmd-push = Push
cmd-pull = Pull
cmd-add-caret-above = Add caret above
cmd-add-caret-below = Add caret below
cmd-select-next-occurrence = Select next occurrence
cmd-toggle-soft-wrap = Soft wrap
cmd-extract-variable = Extract variable
cmd-extract-method = Extract method
cmd-inline-variable = Inline variable
cmd-change-signature = Change signature
cmd-move-class = Move class
cmd-debug-test = Debug test
cmd-toggle-breakpoint = Toggle breakpoint
cmd-resume = Resume program
cmd-step-over = Step over
cmd-step-into = Step into
cmd-step-out = Step out
cmd-stop-debug = Stop debugging
cmd-extend-selection = Extend selection
cmd-shrink-selection = Shrink selection
cmd-join-lines = Join lines
cmd-split-line = Split line
cmd-start-new-line = Start new line
cmd-start-new-line-before = Start new line before current
cmd-complete-statement = Complete current statement
cmd-toggle-case = Toggle case
cmd-transpose = Transpose characters
cmd-block-comment = Comment with block comment
cmd-match-brace = Move caret to matching brace
cmd-block-start = Move caret to code block start
cmd-block-end = Move caret to code block end
cmd-word-completion = Cyclic word completion
cmd-unwrap = Unwrap/Remove
cmd-surround-with = Surround with
cmd-sort-lines = Sort lines
cmd-reverse-lines = Reverse lines
cmd-unique-lines = Remove duplicate lines
cmd-select-all-occurrences = Select all occurrences
cmd-find-action = Find action
cmd-next-error = Next problem
cmd-previous-error = Previous problem
action-find-action = Find Action
action-surround-with = Surround With
notice-no-problems = No problems in this file
cmd-quick-doc = Quick documentation
cmd-toggle-whitespace = Show whitespace
notice-no-documentation = No documentation here
cmd-move-statement-up = Move statement up
cmd-move-statement-down = Move statement down
cmd-move-element-left = Move element left
cmd-move-element-right = Move element right
cmd-delete-word-end = Delete to word end
cmd-delete-word-start = Delete to word start
cmd-add-caret-per-line = Add caret to each selected line
cmd-toggle-bookmark = Toggle bookmark
cmd-show-bookmarks = Show bookmarks
cmd-jump-to-last-edit = Last edit location
cmd-recent-locations = Recent locations
cmd-paste-from-history = Paste from history
cmd-copy-reference = Copy reference
cmd-goto-super-method = Go to super method
cmd-goto-type-declaration = Go to type declaration
cmd-goto-test = Go to test
action-recent-locations = Recent Locations
action-bookmarks = Bookmarks
action-paste-from-history = Paste from History
action-go-to-test = Go to Test
notice-no-bookmarks = No bookmarks yet (F3 adds one)
notice-clipboard-empty = Nothing copied yet
clipboard-lines = { $count } lines
notice-copied = Copied { $text }
notice-no-super-method = No super method
notice-no-type-declaration = No type declaration found
notice-no-test = No test or subject found for { $name }
notify-error = Error
notify-tests = Tests finished
notify-tests-body = { $passed } passed, { $failed } failed
cmd-zoom-in = Zoom in
cmd-zoom-out = Zoom out
cmd-zoom-reset = Reset zoom
notice-zoom = Zoom { $percent }%
cmd-compare-with-clipboard = Compare with clipboard
cmd-local-history = Local history
cmd-annotate = Annotate with Git blame
panel-compare = Compare
compare-clipboard = Clipboard ↔ { $file }
compare-history = Saved version ↔ { $file }
compare-restore = Restore this version
compare-identical = No differences
notice-no-history = No saved versions of this file yet
notice-restored = Version restored (⌘Z undoes)
history-ago = { $age } ago
action-local-history = Local history of { $file }
annotate-uncommitted = Not committed yet
cmd-introduce-constant = Introduce constant
cmd-introduce-field = Introduce field
cmd-introduce-parameter = Introduce parameter
refactor-introduce-constant = Introduce Constant
refactor-introduce-constant-help = Constant name; every occurrence in the class becomes self::NAME
refactor-introduce-field = Introduce Field
refactor-introduce-field-help = Property name; the value is assigned to $this->name before the statement
refactor-introduce-parameter = Introduce Parameter
refactor-introduce-parameter-help = Parameter name; the expression becomes its default value
cmd-quick-definition = Quick definition
cmd-toggle-distraction-free = Distraction-free mode
