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
