# Workbench strings: welcome screen, tool rail, editor chrome, find bar,
# find-in-path dialog, palette, popups and status bar.
# Keys: <area>-<thing>, lowercase kebab-case.

welcome-hint = Open a PHP or Laravel project to start. Indexing, completion and inspections run in-process, nothing to configure.
welcome-open-folder = Open Folder…
welcome-recent = RECENT PROJECTS
welcome-no-recent = Projects you open will show up here.
shortcuts-title = Open a file from the project tree
sidebar-project = PROJECT

rail-project = Project
rail-run-tests = Run tests

action-search-everywhere = Search everywhere
action-go-to-file = Go to file
action-go-to-class = Go to class
action-go-to-symbol = Go to symbol
action-go-to-line = Go to line
action-recent-files = Recent files
action-find-in-path = Find in path
action-file-structure = File structure
action-choose = Choose

palette-placeholder = Type a name
palette-placeholder-line = line[:column]
palette-placeholder-recent = Filter recent files
palette-nothing-found = Nothing found
palette-hints = ↑↓ navigate   ↵ open   esc close
palette-line-item = Line { $line }, column { $column }
palette-symbol-detail = { $owner } · line { $line }

list-position = { $current } of { $total }
completion-keys = ↵ insert · ⇥ replace
status-claude-running = Claude running in { $project }
status-claude-exited = Claude exited

find-placeholder = Find
find-match-case = Match case
find-words = Words
find-regex = Regex
find-bad-pattern = Bad pattern
find-no-results = No results
find-count = { $count ->
    [one] 1 result
   *[other] { $count } results
}
replace-placeholder = Replace
replace-one = Replace
replace-all = Replace All

search-title = Find in Path
search-hints = ↵ search   esc close
search-placeholder = Text to find
search-mask-placeholder = File mask, e.g. *.php
search-submit = Find

rename-placeholder = New name
rename-title = Rename { $name }
rename-hints = ↵ rename   esc cancel
rename-help = Updates every usage in the project; a class file named after the class is renamed too.

status-indexing = Indexing…
status-indexed = { $files ->
    [one] 1 file indexed
   *[other] { $files } files indexed
} · { $elapsed }
status-inspecting = Inspecting…
status-position = Ln { $line }, Col { $column }
gutter-route-action = Go to { $target }
gutter-model-table = Go to the migration of { $table }
gutter-run = Run: { $command }
panel-move-right = Move to the right
panel-move-bottom = Move to the bottom

# Beside dependencies in composer.json and package.json, when the lock file lacks one.
package-not-installed = not installed

# Markdown tab view toggle (tooltips).
markdown-editor = Editor only
markdown-split = Editor and preview
markdown-preview = Preview only
