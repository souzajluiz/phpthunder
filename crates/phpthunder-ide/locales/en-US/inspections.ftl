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
