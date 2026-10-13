# Workbench strings: welcome screen, tool rail, editor chrome, find bar,
# find-in-path dialog, palette, popups and status bar.
# Keys: <area>-<thing>, lowercase kebab-case.

welcome-hint = Open a PHP project folder to start.
welcome-open-folder = Open Folder
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
