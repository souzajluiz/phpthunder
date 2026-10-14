# Bottom-panel strings: tab names, Problems, Usages, Find, Tests, Terminal.
# Keys: <area>-<thing>, lowercase kebab-case.

panel-usages = Usages
panel-find = Find
panel-problems = Problems
panel-hierarchy = Hierarchy
panel-tests = Tests
panel-terminal = Terminal
panel-claude = Claude

problems-no-file = No file open
problems-none = No problems in { $file }

usages-searching = Searching usages of { $name }…
usages-declaration = declaration
usages-title = { $name } — { $count ->
    [one] 1 usage
   *[other] { $count } usages
} in { $files ->
    [one] 1 file
   *[other] { $files } files
}

find-results-searching = Searching for “{ $query }”…
find-results-title = “{ $query }” — { $matches ->
    [one] 1 match
   *[other] { $matches } matches
} in { $files ->
    [one] 1 file
   *[other] { $files } files
}
find-results-truncated = (stopped at the limit)

tests-run-again = Run again
tests-run-file = Run file
tests-run-all = Run all
tests-run-coverage = With coverage
tests-coverage = Coverage { $percent }% of statements
tests-hide-coverage = Hide coverage
tests-none = No test run yet
tests-running = Running { $scope }…
tests-all = All tests
tests-passed = { $count } passed
tests-failed = { $count } failed
tests-skipped = { $count } skipped

terminal-none = No terminal running
terminal-start = Start
terminal-exited = Process exited
terminal-restart = Restart

panel-commit = Commit
panel-debug = Debug
